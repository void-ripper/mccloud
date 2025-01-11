use mcriddle::{IntoTargetAddr, Peer};
use std::time::Duration;

mod utils;

#[tokio::test]
async fn three_peers() {
    let _e = utils::init_log("data/three_peers.log").entered();

    let mut cl = utils::cluster::Cluster::new(0);

    let mut peers = cl.create(3, false);

    tokio::time::sleep(Duration::from_millis(100)).await;

    tracing::debug!("-- connect clients --");

    let p1addr = peers[1].cfg.addr.into_target_addr().unwrap();
    let keep_alive = peers[0].cfg.keep_alive;
    let gather_time = peers[0].cfg.data_gather_time;
    peers[0].connect(p1addr.to_owned()).await.unwrap();
    peers[2].connect(p1addr.to_owned()).await.unwrap();

    tokio::time::sleep(keep_alive).await;

    tracing::debug!("-- check all_known --");

    utils::assert_all_known(&peers, 2).await;

    peers[0].share(b"bunnys gonna bunny".to_vec().into()).await.unwrap();

    tokio::time::sleep(gather_time).await;

    tracing::debug!("-- shutdown --");
    peers[2].shutdown().unwrap();

    tokio::time::sleep(keep_alive).await;

    tracing::debug!("-- check all_known --");
    utils::assert_all_known(&peers, 1).await;

    peers[0].share(b"no, bunnys gonna hop!".to_vec().into()).await.unwrap();

    tokio::time::sleep(gather_time).await;

    peers[2] = Peer::new(peers[2].cfg.clone()).unwrap();
    peers[2].connect(p1addr).await.unwrap();

    tokio::time::sleep(keep_alive).await;

    tracing::debug!("-- check all_known --");
    utils::assert_all_known(&peers, 2).await;

    tokio::time::sleep(Duration::from_millis(200)).await;

    cl.shutdown();
    // cl.cleanup();
}
