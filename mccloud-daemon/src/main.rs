#![doc = include_str!("../../README.md")]

use std::{net::SocketAddr, path::PathBuf};

use clap::Parser;
use mccloud::{config::Config, IntoTargetAddr};

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 29092)]
    port: u16,
    #[arg(long, default_value = "data")]
    data: PathBuf,
    #[arg(long)]
    conn: Vec<String>,
    #[arg(long, default_value = "debug")]
    log: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    tracing_subscriber::fmt().with_env_filter(&args.log).init();

    let cfg = Config {
        addr: SocketAddr::new(args.host.parse().unwrap(), args.port),
        folder: args.data,
        ..Default::default()
    };
    let peer = mccloud::Peer::new(cfg).unwrap();

    for conn in args.conn.iter() {
        match conn.as_str().into_target_addr() {
            Ok(conn) => {
                if let Err(e) = peer.connect(conn.to_owned()).await {
                    tracing::error!("{e}");
                }
            }
            Err(e) => {
                tracing::error!("{} {}", conn, e);
            }
        }
    }

    if let Err(e) = tokio::signal::ctrl_c().await {
        tracing::error!("{e}");
    }
}
