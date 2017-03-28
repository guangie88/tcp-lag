extern crate chashmap;

#[macro_use]
extern crate error_chain;

#[macro_use]
extern crate log;
extern crate log4rs;

#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate structopt;

#[macro_use]
extern crate structopt_derive;

use chashmap::CHashMap;
use std::fs::File;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use structopt::StructOpt;

mod errors {
    error_chain! {
        errors {}
    }
}

use errors::*;

#[derive(Serialize, Deserialize, Debug)]
struct PingerConfig {
    listen_addrs: Vec<SocketAddr>,
    ping_delay: Duration,
}

#[derive(Serialize, Deserialize, Debug)]
struct ListenConfig {
    listen_addr: SocketAddr,
    msg: String,
}

#[derive(Serialize, Deserialize, Debug)]
enum Operation {
    Pinger(PingerConfig),
    Listen(ListenConfig),
}

#[derive(Serialize, Deserialize, Debug)]
struct FileConfig {
    op: Operation,
}

#[derive(StructOpt, Debug)]
#[structopt(name = "Test", about = "Test program")]
struct ArgConfig {
    #[structopt(short = "c", long = "config", help = "File configuration path")]
    config_path: String,

    #[structopt(short = "l", long = "log-config", help = "Log configuration file path")]
    log_config_path: String,
}

fn run_pinger(config: &PingerConfig) -> Result<()> {
    info!("Running pinger with {} listener(s)...", config.listen_addrs.len());

    // spawn equal # of threads to ping and receive response
    let resp_map = Arc::new(CHashMap::<SocketAddr, CHashMap<String, u64>>::new());

    let ts = config.listen_addrs.iter().map(|listen_addr| {
        let resp_map = resp_map.clone();
        let listen_addr = listen_addr.clone();
        let ping_delay = config.ping_delay.clone();

        thread::spawn(move || {
            let resp_map = resp_map;

            loop {
                let res = TcpStream::connect(&listen_addr)
                    .and_then(|mut stream| {
                        let listen_addr = stream.peer_addr()?;

                        let mut msg = String::new();
                        let _ = stream.read_to_string(&mut msg)?;
                        let msg = msg;

                        resp_map.upsert(
                            listen_addr,

                            || {
                                let resp = CHashMap::new();
                                resp.insert(msg.clone(), 1);
                                resp
                            },

                            |resp| {
                                resp.upsert(
                                    msg.clone(),
                                    || 1,
                                    |count| { *count = *count + 1; });
                            });
                        

                        Ok(())
                    });

                if let Err(e) = res {
                    error!(r#"Unable to connect to listener at "{}": {}"#, listen_addr, e);
                }

                let ping_delay = ping_delay.clone();
                thread::sleep(ping_delay);
            }
        })
    });

    for t in ts.into_iter() {
        let _ = t.join();
    }

    Ok(())
}

fn run_listen(config: &ListenConfig) -> Result<()> {
    info!(r#"Running listener at "{}""#, config.listen_addr);

    // thread to keep receiving ping responses
    let listener = TcpListener::bind(&config.listen_addr)
        .chain_err(|| format!(r#"Unable to bind TCP listener at "{}""#, config.listen_addr))?;

    for stream in listener.incoming() {
        let mut stream = stream.chain_err(|| "Unable to get valid incoming stream from listener")?;
        let write_res = stream.write_fmt(format_args!("{}", config.msg));

        match write_res {
            Ok(_) => info!("Successfully sent message: {}", config.msg),
            Err(e) => error!("Error writing listener message back to pinger: {}", e),
        }
    }

    Ok(())
}

fn run() -> Result<()> {
    let arg_config = ArgConfig::from_args();

    let _ = log4rs::init_file(&arg_config.log_config_path, Default::default())
       .chain_err(|| format!(r#"Unable to initialize log4rs logger with the given config file at "{}""#, arg_config.log_config_path))?;

    let config_str = {
        let mut config_file = File::open(&arg_config.config_path)
            .chain_err(|| format!("Unable to open config file path at {:?}", arg_config.config_path))?;

        let mut s = String::new();

        config_file.read_to_string(&mut s)
            .map(|_| s)
            .chain_err(|| "Unable to read config file into string")?
    };

    let config: FileConfig = serde_json::from_str(&config_str)
        .chain_err(|| format!("Unable to parse config as required JSON format: {}", config_str))?;

    info!("Completed configuration initialization!");

    // listen based on the operation type
    
    match config.op {
        Operation::Pinger(ref config) => run_pinger(config),
        Operation::Listen(ref config) => run_listen(config),
    }
}

fn main() {
    match run() {
        Ok(_) => {
            println!("Program completed!");
            process::exit(0)
        },

        Err(ref e) => {
            let stderr = &mut io::stderr();

            writeln!(stderr, "Error: {}", e)
                .expect("Unable to write error into stderr!");

            for e in e.iter().skip(1) {
                writeln!(stderr, "- Caused by: {}", e)
                    .expect("Unable to write error causes into stderr!");
            }

            process::exit(1);
        },
    }
}
