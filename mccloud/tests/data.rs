use mccloud::{IntoTargetAddr, Peer};
use std::time::Duration;

mod utils;

#[tokio::test]
async fn send_single_data() {
    let _span = utils::init_log("data/data_send_single_data.log").entered();

    let mut cl = utils::cluster::Cluster::new(10);

    let peers = cl.create(2, false);
    let clients = cl.create(1, true);

    tokio::time::sleep(Duration::from_millis(200)).await;

    peers[1]
        .connect("127.0.0.1:39103".into_target_addr().unwrap())
        .await
        .unwrap();
    clients[0]
        .connect("127.0.0.1:39103".into_target_addr().unwrap())
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    clients[0].share(b"my data".to_vec().into()).await.unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;

    cl.shutdown();
    cl.cleanup();
}

#[tokio::test]
async fn data_reboot() {
    let _span = utils::init_log("data/data_reboot.log").entered();

    let mut cl = utils::cluster::Cluster::new(0);

    let mut peers = cl.create(1, false);
    let sleep = 100;

    tokio::time::sleep(Duration::from_millis(sleep)).await;

    let cfg = peers[0].cfg.clone();
    peers[0].share(b"bla bla".to_vec().into()).await.unwrap();

    tokio::time::sleep(Duration::from_millis(sleep)).await;

    peers[0].shutdown().unwrap();

    tokio::time::sleep(Duration::from_millis(sleep)).await;

    peers[0] = Peer::new(cfg).unwrap();

    tokio::time::sleep(Duration::from_millis(sleep)).await;

    peers[0].share(b"bla bla".to_vec().into()).await.unwrap();

    tokio::time::sleep(Duration::from_millis(sleep)).await;

    cl.shutdown();
    cl.cleanup();
}
