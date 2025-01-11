use std::{
    collections::{HashMap, HashSet},
    time::{Duration, SystemTime},
};

use mccloud::IntoTargetAddr;
use utils::cluster::Cluster;

mod utils;

async fn mass_testing(cnt: usize, seed: u16) {
    let mut cl = Cluster::new(seed);

    let peers = cl.create(cnt, false);

    tokio::time::sleep(Duration::from_millis(200)).await;

    for i in 1..peers.len() {
        let p0 = &peers[i - 1];
        let p1 = &peers[i];

        p1.connect(p0.cfg.addr.into_target_addr().unwrap()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let keepalive = peers[0].cfg.keep_alive;
    let mut tries = 3;
    let mut all_connected = false;
    let mut check = HashMap::new();

    while !all_connected && tries > 0 {
        tokio::time::sleep(keepalive * 3).await;
        tracing::info!("-- check all connected --");
        check.clear();
        all_connected = true;
        for (i, p) in peers.iter().enumerate() {
            let all_known = p.known_pubkeys().await;
            if all_known.len() != cnt - 1 {
                all_connected = false;
                // break;
                for (j, q) in peers.iter().enumerate() {
                    if i != j && !all_known.contains(&q.pubkey()) {
                        check.entry(i).or_insert_with(|| HashSet::new()).insert(j);
                    }
                }
            }
        }

        tries -= 1;
    }

    if !check.is_empty() {
        tracing::error!("{:?}", check);
    }
    assert!(all_connected);

    let mut rx = peers[1].last_block_receiver();

    let start = SystemTime::now();
    peers[1].share(b"bunny hop".to_vec().into()).await.unwrap();
    match rx.recv().await {
        Ok(blk) => {
            let s = String::from_utf8_lossy(&blk.data[0].data);
            tracing::info!("blk: {}", s);
        }
        Err(e) => tracing::error!("{e}"),
    }
    let dur = start.elapsed().unwrap();
    tracing::warn!("SHARING: {:?}", dur);

    cl.shutdown();
    cl.cleanup();
}

// #[tokio::test]
// async fn mass_line_3() {
//     let _e = utils::init_log("data/mass_line_3.log").entered();

//     mass_testing(3, 0).await;
// }

// #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
// // #[tokio::test]
// async fn mass_line_20() {
//     let _e = utils::init_log("data/mass_line_20.log").entered();

//     mass_testing(20, 10).await;
// }

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
// #[tokio::test]
async fn mass_line_200() {
    let _e = utils::init_log("data/mass_line_200.log").entered();

    mass_testing(200, 30).await;
}

//#[tokio::test(flavor = "multi_thread")]
////#[tokio::test]
//async fn mass_line_2000() {
//    let _e = utils::init_log("data/mass_line_2000.log").entered();
//
//    mass_testing(2000, 1000).await;
//}
