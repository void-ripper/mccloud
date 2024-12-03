use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc::SyncSender,
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

#[derive(Clone)]
pub struct User {
    pub id: i64,
    pub pubkey: [u8; 33],
    pub name: String,
}

pub struct PrivateUser {
    pub user: User,
    pub secret: SecretKey,
}

pub enum Update {
    RoomMessage {
        user: [u8; 33],
        room: String,
        message: String,
    },
}

#[derive(BorshSerialize, BorshDeserialize)]
enum Envelope {
    Calledback { msg: Message, cb: u32 },
    Oneshot { msg: Message },
}

#[derive(BorshSerialize, BorshDeserialize)]
enum Message {
    CreateUser {
        pubkey: [u8; 33],
        name: String,
    },
    CreateRoom {
        pubkey: [u8; 33],
        name: String,
    },
    CreateMessage {
        pubkey: [u8; 33],
        room: String,
        message: String,
    },
    CreatePrivateMessage {
        from: [u8; 33],
        to: [u8; 33],
        message: String,
    },
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
    pub fn new(cfg: Config, tx: SyncSender<Update>) -> Result<Self> {
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
            relationship_time: Duration::from_millis(1_000),
            relationship_count: 3,
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
                    if data.data.as_slice() == b"[genesis]" {
                        continue;
                    }
                    let e: Envelope = borsh::from_slice(&data.data).unwrap();
                    let res = match e {
                        Envelope::Calledback { msg, cb } => match msg {
                            Message::CreateUser { pubkey, name } => {
                                Database::handle_create_user(&db0, &cb0, pubkey, name, cb).await
                            }
                            Message::CreateRoom { pubkey, name } => {
                                Database::handle_create_room(&db0, &cb0, pubkey, name, cb).await
                            }
                            _ => Ok(()),
                        },
                        Envelope::Oneshot { msg } => match msg {
                            Message::CreateMessage { pubkey, room, message } => {
                                Database::handle_create_message(&db0, &tx, pubkey, room, message).await
                            }
                            Message::CreatePrivateMessage { from, to, message } => {
                                Database::handle_create_private_message(&db0, from, to, message).await
                            }
                            _ => Ok(()),
                        },
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

    fn share_oneshot(&self, msg: Message) -> Result<()> {
        let data = ex!(borsh::to_vec(&Envelope::Oneshot { msg }), io);

        let peer = self.peer.clone();
        ex!(self.rt.block_on(async move { peer.share(data).await }), riddle);

        Ok(())
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

    async fn handle_create_message(
        db: &SafeConn,
        tx: &SyncSender<Update>,
        pubkey: [u8; 33],
        room: String,
        message: String,
    ) -> Result<()> {
        let sql = r#"
        INSERT INTO room_message(room_id, user_id, message) 
        VALUES(
            (SELECT id FROM room WHERE name = ?1),
            (SELECT id FROM user WHERE pubkey = ?2),
            ?3
        )"#;
        let db = db.lock().await;
        let mut stmt = ex!(db.prepare_cached(sql), sqlite);
        ex!(stmt.execute((&room, pubkey, &message)), sqlite);

        tx.send(Update::RoomMessage {
            user: pubkey,
            room,
            message,
        });

        Ok(())
    }

    pub fn create_message(&self, puser: &PrivateUser, room: &Room, msg: &str) -> Result<()> {
        ex!(
            self.share_oneshot(Message::CreateMessage {
                pubkey: puser.user.pubkey,
                room: room.name.clone(),
                message: msg.to_string()
            }),
            source
        );

        Ok(())
    }

    async fn handle_create_private_message(db: &SafeConn, from: [u8; 33], to: [u8; 33], message: String) -> Result<()> {
        let sql = r#"
            INSERT INTO user_message(from_user, to_user, message)
            VALUES(
                (SELECT id FROM user WHERE pubkey = ?1),
                (SELECT id FROM user WHERE pubkey = ?2),
                ?3
            )
        "#;
        let db = db.lock().await;
        let mut stmt = ex!(db.prepare_cached(sql), sqlite);
        ex!(stmt.execute((from, to, message)), sqlite);

        Ok(())
    }

    pub fn create_private_message(&self, puser: &PrivateUser, to: [u8; 33], msg: &str) -> Result<()> {
        ex!(
            self.share_oneshot(Message::CreatePrivateMessage {
                from: puser.user.pubkey,
                to,
                message: msg.to_string()
            }),
            source
        );
        Ok(())
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

    pub fn users_in_room(&self, room: &str) -> Result<HashMap<[u8; 33], User>> {
        let db = self.db.clone();
        let users = self.rt.block_on(async move {
            let sql = r#"SELECT u.id, u.pubkey, u.name
            FROM room_message rm, user u
            WHERE 
                rm.room_id = (SELECT id FROM room WHERE name = ?1)
                AND rm.user_id = u.id
            ORDER BY rm.id DESC
            LIMIT 20
            "#;
            let db = db.lock().await;
            let mut stmt = ex!(db.prepare_cached(sql), sqlite);
            let it = ex!(
                stmt.query_map((room,), |r| Ok(User {
                    id: r.get(0)?,
                    pubkey: r.get(1)?,
                    name: r.get(2)?
                })),
                sqlite
            );
            let mut users = HashMap::new();
            for n in it {
                let u = ex!(n, sqlite);
                users.insert(u.pubkey, u);
            }

            Ok(users)
        });

        users
    }

    pub fn last_20_lines(&self, room: &str) -> Result<Vec<([u8; 33], String)>> {
        let db = self.db.clone();
        let msgs: Result<Vec<([u8; 33], String)>> = self.rt.block_on(async move {
            let sql = r#"
            SELECT u.pubkey, rm.message 
            FROM room_message rm, user u
            WHERE 
                rm.room_id = (SELECT r.id FROM room r WHERE r.name = ?1)
                AND rm.user_id = u.id 
            ORDER BY rm.id DESC LIMIT 20
            "#;
            let db = db.lock().await;
            let mut stmt = ex!(db.prepare_cached(sql), sqlite);
            let res = ex!(stmt.query_map((room,), |r| Ok((r.get(0)?, r.get(1)?))), sqlite);

            let mut msgs = Vec::new();
            for n in res {
                msgs.push(ex!(n, sqlite));
            }
            msgs.reverse();

            Ok(msgs)
        });

        msgs
    }
}
