use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

use borsh::{BorshDeserialize, BorshSerialize};
use k256::{
    elliptic_curve::{rand_core::OsRng, sec1::ToEncodedPoint},
    SecretKey,
};
use mcriddle::{self, Peer};
use tokio::{
    runtime::Runtime,
    sync::{oneshot, Mutex},
};

use crate::{
    config::Config,
    error::{Error, Result},
};

macro_rules! ex {
    ($e: expr, $name: tt) => {
        $e.map_err(|e| Error::$name(line!(), module_path!(), e))?
    };
}

pub struct User {
    pub id: i64,
    pub pubkey: [u8; 33],
    pub name: String,
}

pub struct PrivateUser {
    pub user: User,
    pub secret: SecretKey,
}

#[derive(BorshSerialize, BorshDeserialize)]
enum Message {
    CreateUser { pubkey: [u8; 33], name: String, cb: u32 },
    CreatedUser { cb: u32, id: i64 },
}

enum Answer {
    CreatedUser { id: i64 },
}

pub struct Database {
    cfg: Config,
    rt: Runtime,
    peer: Arc<Peer>,
    db: Arc<Mutex<rusqlite::Connection>>,
    cb_pool: AtomicU32,
    callback: Arc<Mutex<HashMap<u32, oneshot::Sender<Answer>>>>,
}

impl Database {
    pub fn new(cfg: Config) -> Result<Self> {
        let dbfile = cfg.data.join("data.db");
        let existed = dbfile.exists();
        let db = ex!(rusqlite::Connection::open(&dbfile), sqlite);
        let db = Arc::new(Mutex::new(db));

        if !existed {}

        let rt = ex!(Runtime::new(), sync);
        let peer_cfg = mcriddle::Config {
            addr: "0.0.0.0:29092".into(),
            folder: cfg.data.clone(),
        };
        let p = ex!(rt.block_on(async { Peer::new(peer_cfg) }), riddle);

        let mut lbr = p.last_block_receiver();
        let db0 = db.clone();
        rt.spawn(async move {
            while let Ok(blk) = lbr.recv().await {
                for data in blk.data {
                    let m: Message = borsh::from_slice(&data).unwrap();
                }
            }
        });

        Ok(Self {
            cfg,
            rt,
            db,
            peer: p,
            cb_pool: AtomicU32::new(0),
            callback: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    async fn send(&self, msg: &Message) -> Result<()> {
        let data = ex!(borsh::to_vec(msg), io);

        ex!(self.peer.share(data).await, riddle);

        Ok(())
    }

    pub fn create_user(&mut self, name: String) -> User {
        let secret = SecretKey::random(&mut OsRng);
        let public = secret.public_key().to_encoded_point(true);
        let public = public.as_bytes();

        let mut user = User {
            id: 0,
            pubkey: [0u8; 33],
            name,
        };
        user.pubkey.copy_from_slice(public);
        let cb = self.cb_pool.fetch_add(1, Ordering::SeqCst);

        let (tx, rx) = oneshot::channel();
        self.rt.spawn(async move {
            self.callback.lock().await.insert(cb, tx);
        });

        let answer = self.rt.block_on(async move {
            let recv = rx.await;
            recv.unwrap()
        });

        if let Answer::CreatedUser { id } = answer {
            user.id = id;
        }

        user
    }
}
