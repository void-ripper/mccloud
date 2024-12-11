use std::{path::PathBuf, sync::Arc, time::Duration};

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use indexmap::IndexMap;
use mcriddle::{Config, Peer};
use serde::{Deserialize, Serialize};
use tokio::{net::TcpListener, sync::Mutex};

type AppPtr = State<Arc<Mutex<App>>>;

struct App {
    peers: IndexMap<String, Arc<Peer>>,
    port_pool: u16,
}

impl App {
    async fn list(&self) -> Vec<PeerData> {
        let mut peers = Vec::new();

        for p in self.peers.values() {
            let data = PeerData {
                id: p.pubhex(),
                connections: p.client_pubkeys().await.into_iter().map(|n| hex::encode(n)).collect(),
                all_known: p.known_pubkeys().await.into_iter().map(|n| hex::encode(n)).collect(),
            };
            peers.push(data);
        }

        peers
    }

    async fn spawn_peers(&mut self, thin: bool, count: u32) -> Vec<PeerData> {
        for _ in 0..count {
            let cfg = Config {
                addr: ([127, 0, 0, 1], self.port_pool).into(),
                folder: PathBuf::from("data").join(self.port_pool.to_string()),
                keep_alive: Duration::from_millis(250),
                data_gather_time: Duration::from_millis(500),
                thin,
                relationship_time: Duration::from_millis(5000),
                relationship_count: 2,
            };
            self.port_pool += 1;

            let p = Peer::new(cfg);

            match p {
                Ok(p) => {
                    self.peers.insert(p.pubhex(), p);
                }
                Err(e) => {
                    tracing::error!("{e}");
                }
            }
        }

        self.list().await
    }

    fn delete_peer(&mut self, pubkey: &str) {
        if let Some(v) = self.peers.swap_remove(pubkey) {
            if let Err(e) = v.shutdown() {
                tracing::error!("{e}");
            }
        }
    }
}

#[derive(Serialize)]
pub struct PeerData {
    id: String,
    connections: Vec<String>,
    all_known: Vec<String>,
}

async fn peer_list(state: AppPtr) -> Json<Vec<PeerData>> {
    let app = state.lock().await;
    Json(app.list().await)
}

#[derive(Deserialize)]
pub struct CreateData {
    pub thin: bool,
    pub count: u32,
}

async fn peer_create(state: AppPtr, data: Json<CreateData>) -> Json<Vec<PeerData>> {
    let data = state.lock().await.spawn_peers(data.thin, data.count).await;
    Json(data)
}

async fn peer_shutdown(state: AppPtr, Path(pubhex): Path<String>) -> Json<Vec<PeerData>> {
    let mut app = state.lock().await;
    app.delete_peer(&pubhex);
    Json(app.list().await)
}

#[derive(Deserialize)]
pub struct Share {
    pub id: String,
    pub msg: String,
}

async fn peer_share(state: AppPtr, Json(share): Json<Share>) {
    let app = state.lock().await;
    if let Some(p) = app.peers.get(&share.id) {
        if let Err(e) = p.share(share.msg.into_bytes()).await {
            tracing::error!("{e}");
        }
    }
}

#[derive(Deserialize)]
pub struct ConnData {
    pub frm: String,
    pub to: String,
}

async fn peer_connect(state: AppPtr, Json(conn): Json<ConnData>) {
    let app = state.lock().await;
    if let Some((frm, to)) = app.peers.get(&conn.frm).zip(app.peers.get(&conn.to)) {
        if let Err(e) = frm.connect(to.cfg.addr).await {
            tracing::error!("{e}");
        }
    }
}

#[derive(Serialize)]
pub struct BlockData {
    pub hash: String,
    pub data: Vec<String>,
}

async fn peer_blocks(state: AppPtr, Path(pubhex): Path<String>) -> Json<Vec<BlockData>> {
    let app = state.lock().await;
    let mut blocks = Vec::new();

    if let Some(p) = app.peers.get(&pubhex) {
        for blk in p.block_iter().await {
            let b = blk.unwrap();
            let data: Vec<String> = b.data.into_iter().map(|d| String::from_utf8(d.data).unwrap()).collect();
            blocks.push(BlockData {
                hash: hex::encode(b.hash),
                data,
            });
        }
    }

    Json(blocks)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_env_filter("mcriddle=debug").init();

    let app = App {
        peers: IndexMap::new(),
        port_pool: 29102,
    };

    let router = Router::new()
        .route("/api/list", get(peer_list))
        .route("/api/create", post(peer_create))
        .route("/api/shutdown/:pubhex", post(peer_shutdown))
        .route("/api/share", post(peer_share))
        .route("/api/connect", post(peer_connect))
        .route("/api/blocks/:pubhex", post(peer_blocks))
        .with_state(Arc::new(Mutex::new(app)));

    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, router).await.unwrap();
}
