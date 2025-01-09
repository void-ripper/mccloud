use mcriddle::IntoTargetAddr;
use std::time::Duration;

mod utils;

#[tokio::test]
async fn five_peers() {
    let _e = utils::init_log("data/five_peers.log").entered();

    let mut cl = utils::cluster::Cluster::new(0);

    let three = cl.create(3, false);
    let two = cl.create(2, false);

    tokio::time::sleep(Duration::from_millis(100)).await;

    let keepalive = three[0].cfg.keep_alive;

    three[0]
        .connect(three[1].cfg.addr.into_target_addr().unwrap())
        .await
        .unwrap();
    three[2]
        .connect(three[1].cfg.addr.into_target_addr().unwrap())
        .await
        .unwrap();

    two[1]
        .connect(two[0].cfg.addr.into_target_addr().unwrap())
        .await
        .unwrap();

    tokio::time::sleep(keepalive).await;

    utils::assert_all_known(&three, 2).await;
    utils::assert_all_known(&two, 1).await;

    three[1]
        .connect(two[0].cfg.addr.into_target_addr().unwrap())
        .await
        .unwrap();

    let mut five = Vec::new();
    five.extend(three.iter().cloned());
    five.extend(two.iter().cloned());

    tokio::time::sleep(keepalive).await;

    utils::assert_all_known(&five, 4).await;

    cl.shutdown();
    cl.cleanup();
}
