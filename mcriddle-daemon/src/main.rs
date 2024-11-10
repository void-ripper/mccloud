use std::{path::PathBuf, time::Duration};

use clap::Parser;

#[derive(Parser)]
#[command(version)]
struct Args {
    #[arg(long, default_value_t = 39093)]
    port: u16,
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
    let peer00 = mcriddle::Peer::new(cfg).unwrap();
    let cfg = mcriddle::Config {
        addr: "127.0.0.1:39094".into(),
        folder: PathBuf::from("data1"),
    };
    let peer01 = mcriddle::Peer::new(cfg).unwrap();

    peer00.connect("127.0.0.1:39094".parse().unwrap()).await;

    std::thread::sleep(Duration::from_secs(1));

    peer01.share(b"hello".to_vec()).await;

    std::thread::sleep(Duration::from_secs(1));

    peer01.shutdown();

    std::thread::sleep(Duration::from_secs(2));
}
