use std::{
    io::{Cursor, Write},
    time::SystemTime,
};

use bytesize::ByteSize;
use k256::{elliptic_curve::sec1::ToEncodedPoint, SecretKey};
use mccloud::{
    highlander::{Game, Highlander},
    PubKeyBytes,
};
use rand::rngs::OsRng;
use rayon::prelude::*;

mod utils;

fn create_key() -> (PubKeyBytes, SecretKey) {
    let private = SecretKey::random(&mut OsRng);
    let public = private.public_key().to_encoded_point(true);
    let bytes = public.to_bytes();
    let mut pubkey = [0u8; 33];
    pubkey.copy_from_slice(&bytes);
    (pubkey, private)
}

#[test]
fn even_numbered() {
    let _e = utils::init_log("data/highlander_even_numbered.log").entered();

    let mut hl0 = Highlander::new();
    let mut hl1 = Highlander::new();
    let keys = vec![create_key(), create_key()];
    let pubkeys: Vec<PubKeyBytes> = keys.iter().map(|n| n.0).collect();

    hl0.populate_roster(pubkeys.iter());
    hl1.populate_roster(pubkeys.iter());

    let game = hl0.create_game(&keys[0].0, &keys[0].1).unwrap();

    hl0.add_game(game.clone());
    hl1.add_game(game);

    assert!(!hl0.is_filled() && !hl1.is_filled());

    let game = hl0.create_game(&keys[1].0, &keys[1].1).unwrap();

    hl0.add_game(game.clone());
    hl1.add_game(game);

    assert!(hl0.is_filled() && hl1.is_filled());

    let res0 = hl0.evaluate();
    let res1 = hl1.evaluate();

    for (id, choices) in hl0.roster.iter() {
        tracing::info!("{} {:?}", hex::encode(id), choices);
    }

    assert_eq!(res0, res1);
}

#[test]
fn odd_numbered() {
    let _e = utils::init_log("data/highlander_odd_numbered.log").entered();

    let mut hl0 = Highlander::new();
    let mut hl1 = Highlander::new();
    let mut hl2 = Highlander::new();
    let keys = vec![create_key(), create_key(), create_key()];
    let pubkeys: Vec<PubKeyBytes> = keys.iter().map(|k| k.0).collect();

    hl0.populate_roster(pubkeys.iter());
    hl1.populate_roster(pubkeys.iter());
    hl2.populate_roster(pubkeys.iter());

    let games: Vec<Game> = keys.iter().map(|k| hl0.create_game(&k.0, &k.1).unwrap()).collect();

    for g in games {
        hl0.add_game(g.clone());
        hl1.add_game(g.clone());
        hl2.add_game(g);
    }

    assert!(hl0.is_filled());
    assert!(hl1.is_filled());
    assert!(hl2.is_filled());

    let r0 = hl0.evaluate();
    let r1 = hl1.evaluate();
    let r2 = hl2.evaluate();

    assert!(r0 == r1 && r0 == r2);

    for (id, choices) in hl0.roster.iter() {
        tracing::info!(
            "{} {:?} {}",
            hex::encode(id),
            choices,
            std::mem::size_of_val(choices.as_slice())
        );
    }
}

fn compress_zst(data: &Vec<u8>, lvl: i32) {
    let start = SystemTime::now();
    // let compressed = zstd::stream::encode_all(&*data, 1).unwrap();
    let cursor = Cursor::new(Vec::new());
    let mut encoder = zstd::Encoder::new(cursor, lvl).unwrap();
    encoder.write_all(data).unwrap();
    encoder.flush().unwrap();
    let compressed = encoder.finish().unwrap().into_inner();
    let dur = start.elapsed().unwrap();
    tracing::info!("zstd -> lvl({}) {:?} {}", lvl, dur, ByteSize(compressed.len() as _));
}

#[test]
fn nine() {
    let _e = utils::init_log("data/highlander_nine.log").entered();

    let num = 9;
    let mut hls: Vec<Highlander> = (0..num).into_iter().map(|_| Highlander::new()).collect();
    let keys: Vec<(PubKeyBytes, SecretKey)> = (0..num).into_iter().map(|_| create_key()).collect();
    let pubkeys: Vec<PubKeyBytes> = keys.iter().map(|k| k.0).collect();

    hls.par_iter_mut().for_each(|h| h.populate_roster(pubkeys.iter()));

    let games: Vec<Game> = (0..num)
        .into_iter()
        .map(|i| hls[i].create_game(&keys[i].0, &keys[i].1).unwrap())
        .collect();

    hls.par_iter_mut().for_each(|h| {
        for g in games.iter() {
            h.add_game(g.clone());
        }
    });

    for h in &hls {
        assert!(h.is_filled());
    }

    tracing::info!("choices {:?}", games[0].rounds);
    let res: Vec<PubKeyBytes> = (0..num).into_iter().map(|i| hls[i].evaluate()).collect();

    let mut all_equal = true;
    for i in 1..num {
        if res[i] != res[0] {
            all_equal = false;
            break;
        }
    }
    assert!(all_equal);

    for (id, choices) in hls[0].roster.iter() {
        tracing::info!(
            "{} {:?} {}",
            hex::encode(id),
            choices,
            std::mem::size_of_val(choices.as_slice())
        );
    }
}

#[test]
fn high_number() {
    let _e = utils::init_log("data/highlander_high_number.log").entered();

    let mut hl = Highlander::new();
    //let num = 1024 * 1024;
    let num = 1024;

    tracing::info!("count: {}", num);

    let start = SystemTime::now();
    let keys: Vec<(PubKeyBytes, SecretKey)> = (0..num).into_par_iter().map(|_| create_key()).collect();
    let dur = start.elapsed().unwrap();
    tracing::info!("key creation -> {:?}", dur);

    let pubkeys: Vec<PubKeyBytes> = keys.iter().map(|k| k.0).collect();

    let start = SystemTime::now();
    hl.populate_roster(pubkeys.iter());
    // hl.par_populate_roster(pubkeys.par_iter());
    let dur = start.elapsed().unwrap();
    tracing::info!("populate -> {:?}", dur);

    let start = SystemTime::now();
    let game = hl.create_game(&keys[0].0, &keys[0].1).unwrap();
    let dur = start.elapsed().unwrap();
    tracing::info!("game creation -> {:?}", dur);

    let start = SystemTime::now();
    let games: Vec<Game> = keys
        .par_iter()
        .skip(1)
        .map(|k| hl.create_game(&k.0, &k.1).unwrap())
        .collect();
    let dur = start.elapsed().unwrap();
    tracing::info!("all game creation -> {:?}", dur);

    let start = SystemTime::now();
    hl.add_game(game);
    let dur = start.elapsed().unwrap();
    tracing::info!("game add -> {:?}", dur);

    let start = SystemTime::now();
    for g in games {
        hl.add_game(g);
    }
    let dur = start.elapsed().unwrap();
    tracing::info!("all game add -> {:?}", dur);

    assert!(hl.is_filled());

    let start = SystemTime::now();
    let re = hl.evaluate();
    let dur = start.elapsed().unwrap();
    tracing::info!("evaluate -> {:?}", dur);
    // let res: Vec<GameResult> = keys.iter().skip(1).map(|k| hl.evaluate(k)).collect();

    tracing::info!("roster bytes {}", ByteSize(std::mem::size_of_val(&hl.roster) as u64));

    let start = SystemTime::now();
    let data = borsh::to_vec(&re).unwrap();
    let dur = start.elapsed().unwrap();
    tracing::info!("borsh -> {:?} {}", dur, ByteSize(data.len() as _));

    for lvl in [1, 3, 9] {
        compress_zst(&data, lvl);
    }
}
