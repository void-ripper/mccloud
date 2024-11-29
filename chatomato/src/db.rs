use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
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

pub struct Room {
    pub id: i64,
    pub name: String,
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
enum Envelope {
    Calledback { msg: Message, cb: u32 },
    Oneshot { msg: Message },
}

#[derive(BorshSerialize, BorshDeserialize)]
enum Message {
    CreateUser { pubkey: [u8; 33], name: String },
    CreateRoom { pubkey: [u8; 33], name: String },
}

enum Answer {
    CreatedUser { id: i64 },
    CreatedRoom { id: i64 },
}

type SafeConn = Arc<Mutex<rusqlite::Connection>>;
type Callbacks = Arc<Mutex<HashMap<u32, oneshot::Sender<Answer>>>>;

pub struct Database {
    pub cfg: Config,
    rt: Runtime,
    peer: Arc<Peer>,
    db: SafeConn,
    cb_pool: AtomicU32,
    callback: Callbacks,
}

impl Database {
    pub fn new(cfg: Config) -> Result<Self> {
        let dbfile = cfg.data.join("data.db");
        let existed = dbfile.exists();
        let db = ex!(rusqlite::Connection::open(&dbfile), sqlite);

        if !existed {
            ex!(db.execute_batch(include_str!("schema.sql")), sqlite);
        }
        let db = Arc::new(Mutex::new(db));

        let rt = ex!(Runtime::new(), sync);
        let peer_cfg = mcriddle::Config {
            addr: cfg.addr.clone().into(),
            folder: cfg.data.clone(),
            keep_alive: Duration::from_millis(250),
            data_gather_time: Duration::from_millis(500),
            thin: true,
        };
        let p = ex!(
            rt.block_on(async {
                let p = ex!(Peer::new(peer_cfg), riddle);
                for conn in cfg.clients.iter() {
                    ex!(p.connect(conn.parse().unwrap()).await, riddle);
                }

                Ok(p)
            }),
            source
        );

        let callback = Arc::new(Mutex::new(HashMap::new()));
        let mut lbr = p.last_block_receiver();
        let db0 = db.clone();
        let cb0 = callback.clone();
        rt.spawn(async move {
            while let Ok(blk) = lbr.recv().await {
                for data in blk.data {
                    if data.as_slice() == b"[genesis]" {
                        continue;
                    }
                    let e: Envelope = borsh::from_slice(&data).unwrap();
                    let res = match e {
                        Envelope::Calledback { msg, cb } => match msg {
                            Message::CreateUser { pubkey, name } => {
                                Database::handle_create_user(&db0, &cb0, pubkey, name, cb).await
                            }
                            Message::CreateRoom { pubkey, name } => {
                                Database::handle_create_room(&db0, &cb0, pubkey, name, cb).await
                            }
                        },
                        Envelope::Oneshot { msg } => Ok(()),
                    };

                    if let Err(e) = res {
                        tracing::error!("{e}");
                    }
                }
            }
        });

        Ok(Self {
            cfg,
            rt,
            db,
            peer: p,
            cb_pool: AtomicU32::new(0),
            callback,
        })
    }

    fn share(&self, msg: Message) -> Result<Answer> {
        let cb = self.cb_pool.fetch_add(1, Ordering::SeqCst);
        let data = ex!(borsh::to_vec(&Envelope::Calledback { msg, cb }), io);

        let (tx, rx) = oneshot::channel();
        let callbacks = self.callback.clone();
        let peer = self.peer.clone();
        ex!(
            self.rt.block_on(async move {
                callbacks.lock().await.insert(cb, tx);
                peer.share(data).await
            }),
            riddle
        );

        let answer = ex!(
            self.rt.block_on(async move {
                let recv = rx.await;
                recv
            }),
            sync
        );

        Ok(answer)
    }

    async fn handle_create_user(db: &SafeConn, cbs: &Callbacks, pubkey: [u8; 33], name: String, cb: u32) -> Result<()> {
        tracing::debug!("handle create user {} {}", name, cb);

        let id = {
            let db = db.lock().await;
            let mut stmt = ex!(
                db.prepare_cached("INSERT INTO user(pubkey, name) VALUES(?1, ?2) "),
                sqlite
            );
            let id = ex!(stmt.insert((pubkey, name)), sqlite);
            id
        };

        let mut cbs = cbs.lock().await;
        if let Some(tx) = cbs.remove(&cb) {
            let _ = tx.send(Answer::CreatedUser { id });
        }

        Ok(())
    }

    pub fn create_user(&self, name: String) -> Result<PrivateUser> {
        let secret = SecretKey::random(&mut OsRng);
        let public = secret.public_key().to_encoded_point(true);
        let public = public.as_bytes();

        tracing::info!("create user {}", name);
        let mut user = User {
            id: 0,
            pubkey: [0u8; 33],
            name,
        };
        user.pubkey.copy_from_slice(public);
        let answer = ex!(
            self.share(Message::CreateUser {
                pubkey: user.pubkey,
                name: user.name.clone(),
            }),
            source
        );

        if let Answer::CreatedUser { id } = answer {
            tracing::info!("use id {}", id);
            user.id = id;
        }

        ex!(std::fs::write(self.cfg.data.join("private.key"), secret.to_bytes()), io);

        Ok(PrivateUser { user, secret })
    }

    async fn handle_create_room(db: &SafeConn, cbs: &Callbacks, pubkey: [u8; 33], name: String, cb: u32) -> Result<()> {
        let id = {
            let db = db.lock().await;
            let mut stmt = ex!(
                db.prepare_cached(
                    "INSERT INTO room(creator_id, name) VALUES((SELECT id FROM user WHERE pubkey = ?1), ?2) "
                ),
                sqlite
            );
            let id = ex!(stmt.insert((pubkey, name)), sqlite);
            id
        };

        let mut cbs = cbs.lock().await;
        if let Some(tx) = cbs.remove(&cb) {
            let _ = tx.send(Answer::CreatedRoom { id });
        }

        Ok(())
    }

    pub fn create_room(&self, name: String, pubkey: [u8; 33]) -> Result<Room> {
        let mut room = Room {
            id: 0,
            name: name.clone(),
        };
        let answer = ex!(self.share(Message::CreateRoom { pubkey, name }), source);

        if let Answer::CreatedRoom { id } = answer {
            room.id = id;
        }

        Ok(room)
    }

    pub fn list_rooms(&self) -> Result<Vec<Room>> {
        let db = self.db.clone();
        let rooms = self.rt.block_on(async move {
            let db = db.lock().await;
            let mut stmt = ex!(db.prepare_cached("SELECT id, name FROM room"), sqlite);
            let res = stmt
                .query_map([], |r| {
                    Ok(Room {
                        id: r.get(0)?,
                        name: r.get(1)?,
                    })
                })
                .unwrap();

            let mut rooms = Vec::new();
            for r in res {
                rooms.push(r.unwrap());
            }

            Ok(rooms)
        });
        rooms
    }

    pub fn user_by_key(&self, pubkey: [u8; 33]) -> Result<User> {
        let db = self.db.clone();
        let user = self.rt.block_on(async move {
            let db = db.lock().await;
            let mut stmt = ex!(db.prepare_cached("SELECT id, name FROM user WHERE pubkey = ?"), sqlite);
            let user = ex!(
                stmt.query_row([pubkey], |r| {
                    Ok(User {
                        id: r.get(0)?,
                        pubkey: pubkey,
                        name: r.get(1)?,
                    })
                }),
                sqlite
            );
            Ok(user)
        });

        user
    }
}
