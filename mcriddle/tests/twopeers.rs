use mcriddle::IntoTargetAddr;
use std::time::Duration;

mod utils;

#[tokio::test]
async fn two_peers() {
    let _e = utils::init_log("data/two_peers.log").entered();

    let mut cl = utils::cluster::Cluster::new(0);

    let peers = cl.create(2, false);
    let clients = cl.create(1, true);

    tokio::time::sleep(Duration::from_millis(200)).await;

    peers[1]
        .connect("127.0.0.1:39093".into_target_addr().unwrap())
        .await
        .unwrap();
    clients[0]
        .connect("127.0.0.1:39093".into_target_addr().unwrap())
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    utils::assert_all_known(&peers, 2).await;

    cl.shutdown();
    cl.cleanup();
}

#[tokio::test]
async fn two_peers_diff_root() {
    let _e = utils::init_log("data/two_peers_diff_root.log").entered();

    let mut cl = utils::cluster::Cluster::new(10);

    let peers = cl.create(2, false);

    tokio::time::sleep(Duration::from_millis(100)).await;

    peers[0].share("v0".into()).await.unwrap();
    peers[1].share("v1".into()).await.unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    peers[0]
        .connect(peers[1].cfg.addr.into_target_addr().unwrap())
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    utils::assert_all_known(&peers, 0).await;

    cl.shutdown();
    cl.cleanup();
}
