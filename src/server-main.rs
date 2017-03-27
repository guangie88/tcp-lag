#[macro_use]
extern crate error_chain;

#[macro_use]
extern crate log;
extern crate log4rs;

#[macro_use]
extern crate serde_derive;
extern crate structopt;

#[macro_use]
extern crate structopt_derive;
extern crate toml;

use std::fs::File;
use std::io::{self, Read, Write};
use std::net::{SocketAddrV4, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process;
use structopt::StructOpt;

mod errors {
    error_chain! {
        errors {
            // ClientMapRead {
            //     description("error in reading client map")
            //     display("error in reading client map")
            // }
        }
    }
}

use errors::*;

mod common;
use common::HANDSHAKE_STR;

#[derive(Serialize, Deserialize, Debug)]
struct FileConfig {
    listener_socket: SocketAddrV4,
    stream_count: usize,
    stream_start_port: u16,
    read_dir: PathBuf,
}

#[derive(StructOpt, Debug)]
#[structopt(name = "Test", about = "Test program")]
struct ArgConfig {
    #[structopt(short = "c", long = "config", help = "File configuration path")]
    config_path: String,

    #[structopt(short = "l", long = "log-config", help = "Log configuration file path")]
    log_config_path: String,
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

    let config: FileConfig = toml::from_str(&config_str)
        .chain_err(|| format!("Unable to parse config as required toml format: {}", config_str))?;

    info!("Completed configuration initialization!");

    let listener = TcpListener::bind(&config.listener_socket)
        .chain_err(|| format!(r#"Unable to create TCP listener at "{}""#, config.listener_socket))?;

    info!("Directory to read from: {:?}", config.read_dir);
    info!(r#"TCP-lag server started listening at "{}"..."#, config.listener_socket);

    let _ = listener.incoming()
        .any(|stream| {
            match stream {
                Ok(mut stream) => {
                    info!("Stream connected, waiting client to send the handshake preamble...");

                    let mut buf = String::new();
                    let read_res = stream.read_to_string(&mut buf);
                    let buf = buf.trim_right();

                    match read_res {
                        Ok(_) => {
                            let res = buf == HANDSHAKE_STR;

                            if res {
                                info!("Found the handshaking preamble!");
                            } else {
                                error!("Invalid handshaking preamble, found: {}", buf);
                            }

                            res
                        },
                        Err(e) => {
                            error!("TCP stream read error: {}", e);
                            false
                        },
                    }
                },

                Err(e) => {
                    error!("TCP stream error: {}", e);
                    false
                },
            }
        });
    
    Ok(())
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
