use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(version)]
struct Args {
    #[arg(long, default_value_t = 29092)]
    port: u16,
    #[arg(long, default_value_t = 29091)]
    wsport: u16,
    #[arg(long)]
    conn: Vec<String>,
    #[arg(long, default_value = "debug")]
    log: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    tracing_subscriber::fmt().with_env_filter(&args.log).init();

    let cfg = mcriddle::Config {
        addr: "127.0.0.1:39093".into(),
        folder: PathBuf::from("data0"),
    };
    let peer = mcriddle::Peer::new(cfg).unwrap();
}
