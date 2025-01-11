use mcriddle::{IntoTargetAddr, Peer};
use std::time::Duration;

mod utils;

#[tokio::test]
async fn reconnect_1() {
    let _e = utils::init_log("data/reconnect_1.log").entered();

    let mut cl = utils::cluster::Cluster::new(0);

    let mut peers = cl.create(2, false);

    tokio::time::sleep(Duration::from_millis(250)).await;

    let keepalive = peers[0].cfg.keep_alive;
    peers[1]
        .connect(peers[0].cfg.addr.into_target_addr().unwrap())
        .await
        .unwrap();
    tokio::time::sleep(keepalive).await;

    peers[0].shutdown().unwrap();
    tokio::time::sleep(keepalive * 2).await;

    let all_kn_cnt01 = peers[1].known_pubkeys().await.len();

    assert_eq!(all_kn_cnt01, 0);

    peers[0] = Peer::new(peers[0].cfg.clone()).unwrap();

    tokio::time::sleep(Duration::from_secs(20)).await;

    let all_kn_cnt00 = peers[0].known_pubkeys().await.len();
    let all_kn_cnt01 = peers[1].known_pubkeys().await.len();

    assert_eq!(all_kn_cnt00, 1);
    assert_eq!(all_kn_cnt01, 1);

    cl.shutdown();
    cl.cleanup();
}

#[tokio::test]
async fn reconnect_2() {
    let _e = utils::init_log("data/reconnect_2.log").entered();

    let mut cl = utils::cluster::Cluster::new(10);

    let mut peers = cl.create(2, false);

    tokio::time::sleep(Duration::from_millis(250)).await;

    let keepalive = peers[0].cfg.keep_alive;
    peers[1]
        .connect(peers[0].cfg.addr.into_target_addr().unwrap())
        .await
        .unwrap();
    tokio::time::sleep(keepalive).await;

    peers[1].shutdown().unwrap();
    tokio::time::sleep(keepalive * 2).await;

    tracing::info!("-- get known count --");
    let all_kn_cnt00 = peers[0].known_pubkeys().await.len();
    //let all_kn_cnt01 = peers[1].known_pubkeys().await.len();

    assert_eq!(all_kn_cnt00, 0);
    //assert_eq!(all_kn_cnt01, 0);

    tracing::info!("-- start listening again --");
    peers[1] = Peer::new(peers[1].cfg.clone()).unwrap();
    tokio::time::sleep(keepalive).await;

    peers[1]
        .connect(peers[0].cfg.addr.into_target_addr().unwrap())
        .await
        .unwrap();

    tokio::time::sleep(keepalive * 2).await;

    let all_kn_cnt00 = peers[0].known_pubkeys().await.len();
    let all_kn_cnt01 = peers[1].known_pubkeys().await.len();

    assert_eq!(all_kn_cnt00, 1);
    assert_eq!(all_kn_cnt01, 1);

    cl.shutdown();
    cl.cleanup();
}
