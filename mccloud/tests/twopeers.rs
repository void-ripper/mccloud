use mccloud::IntoTargetAddr;
use std::time::Duration;

mod utils;

#[tokio::test]
async fn two_peers() {
    let _e = utils::init_log("data/two_peers.log").entered();

    let mut cl = utils::cluster::Cluster::new(0);

    let peers = cl.create(2, false);
    let clients = cl.create(1, true);

    tokio::time::sleep(Duration::from_millis(200)).await;

    let addr = peers[0].cfg.addr.into_target_addr().unwrap();
    peers[1].connect(addr.to_owned()).await.unwrap();
    clients[0].connect(addr.to_owned()).await.unwrap();

    tokio::time::sleep(peers[0].cfg.keep_alive * 2).await;

    utils::assert_all_known(&peers, 1).await;

    cl.shutdown();
    cl.cleanup();
}
