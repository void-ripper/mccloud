use askama::Template;
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Form, Json, Router,
};
use indexmap::IndexMap;
use mccloud::{
    config::{Algorithm, Config, Relationship},
    Peer, TargetAddr,
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::Mutex};
use tower_http::trace::TraceLayer;

mod site;

type AppPtr = State<Arc<Mutex<App>>>;

#[derive(Serialize)]
pub struct PeerData {
    id: String,
    connections: Vec<String>,
    all_known: Vec<String>,
}

impl PeerData {
    pub async fn from(p: &Arc<Peer>) -> Self {
        PeerData {
            id: p.pubhex(),
            all_known: p.known_pubkeys().await.into_iter().map(hex::encode).collect(),
            connections: p.client_pubkeys().await.into_iter().map(hex::encode).collect(),
        }
    }
}

pub struct BlockData {
    pub hash: String,
    pub data: Vec<String>,
    pub author: String,
    pub next_authors: Vec<String>,
}

struct App {
    peers: IndexMap<String, Arc<Peer>>,
    flakies: IndexMap<String, Arc<Peer>>,
    port_pool: u16,
    site: site::IndexSite,
}

impl App {
    async fn update_peer_list(&mut self) {
        let mut peers = Vec::new();

        for p in self.peers.values() {
            let data = PeerData::from(p).await;
            peers.push(data);
        }

        self.site.peers = peers;
    }

    async fn spawn_peers(&mut self, count: u32) {
        let mut new_ones = Vec::new();
        for _ in 0..count {
            let cfg = Config {
                addr: ([127, 0, 0, 1], self.port_pool).into(),
                proxy: None,
                folder: PathBuf::from("data").join(self.port_pool.to_string()),
                data_gather_time: Duration::from_millis(800),
                thin: false,
                relationship: Relationship {
                    time: Duration::from_millis(1000),
                    reconnect: Duration::from_millis(2000),
                    count: 2,
                    retry: 3,
                },
                algorithm: Algorithm::Riddle {
                    next_candidates: 3,
                    forced_restart: true,
                },
            };
            self.port_pool += 1;

            let p = Peer::new(cfg);

            match p {
                Ok(p) => {
                    new_ones.push(p.clone());
                    self.peers.insert(p.pubhex(), p);
                }
                Err(e) => {
                    tracing::error!("{e}");
                }
            }
        }

        for p in new_ones {
            let data = PeerData::from(&p).await;
            self.site.peers.push(data);
        }
    }

    fn delete_peer(&mut self, pubkey: &str) {
        if let Some(v) = self.peers.swap_remove(pubkey) {
            if let Err(e) = v.shutdown() {
                tracing::error!("on shutown {e}");
            }
            std::fs::remove_dir_all(&v.cfg.folder).unwrap();
            self.site.peers.retain(|n| n.id != pubkey);
        }
        if self.peers.capacity() > self.peers.len() * 2 {
            self.peers.shrink_to_fit();
        }
    }
}

async fn index(state: AppPtr) -> impl IntoResponse {
    let app = state.lock().await;
    Html(app.site.render().unwrap())
}

#[derive(Deserialize)]
pub struct FormData {
    pub spawn_count: u32,
    pub msg_to_share: String,
    pub flake_time: u32,
    pub flakies: u32,
}

async fn peer_create(state: AppPtr, data: Form<FormData>) -> impl IntoResponse {
    let mut app = state.lock().await;
    app.site.spawn_count = data.spawn_count;
    app.spawn_peers(data.spawn_count).await;
    Redirect::to("/")
}

async fn peer_select(state: AppPtr, Path(pubhex): Path<String>) -> impl IntoResponse {
    let mut app = state.lock().await;
    let sel = app.peers.get(&pubhex).or(app.flakies.get(&pubhex));

    if let Some(sel) = sel {
        app.site.target = Some(PeerData::from(sel).await);
    }

    Redirect::to("/")
}

async fn circle_connect(state: AppPtr) -> impl IntoResponse {
    let app = state.lock().await;

    for i in 1..app.peers.len() {
        let p0 = &app.peers[i - 1];
        let p1 = &app.peers[i];

        if let Err(e) = p0.connect(TargetAddr::Ip(p1.cfg.addr)).await {
            tracing::error!("{e}");
        }
    }

    let p0 = &app.peers[app.peers.len() - 1];
    let p1 = &app.peers[0];

    if let Err(e) = p0.connect(TargetAddr::Ip(p1.cfg.addr)).await {
        tracing::error!("{e}");
    }

    Redirect::to("/")
}

async fn peer_shutdown(state: AppPtr, Path(pubhex): Path<String>) -> impl IntoResponse {
    let mut app = state.lock().await;
    app.delete_peer(&pubhex);
    app.site.target.take_if(|n| n.id == pubhex);
    Redirect::to("/")
}

async fn peer_share(state: AppPtr, Form(share): Form<FormData>) -> impl IntoResponse {
    let app = state.lock().await;
    if let Some(target) = &app.site.target {
        let sel = app.peers.get(&target.id).or(app.flakies.get(&target.id));
        if let Some(p) = sel {
            if let Err(e) = p.share(share.msg_to_share.into_bytes()).await {
                tracing::error!("{e}");
            }
        }
    }

    Redirect::to("/")
}

#[derive(Deserialize)]
pub struct ConnData {
    pub frm: String,
    pub to: String,
}

async fn peer_connect(state: AppPtr, Json(conn): Json<ConnData>) {
    let app = state.lock().await;
    if let Some((frm, to)) = app.peers.get(&conn.frm).zip(app.peers.get(&conn.to)) {
        if let Err(e) = frm.connect(TargetAddr::Ip(to.cfg.addr)).await {
            tracing::error!("{e}");
        }
    }
}

async fn peer_blocks(state: AppPtr, Path(pubhex): Path<String>) -> Json<Vec<BlockData>> {
    let app = state.lock().await;
    let mut blocks = Vec::new();

    if let Some(p) = app.peers.get(&pubhex) {
        for blk in p.block_iter().await {
            let b = blk.unwrap();
            let data: Vec<String> = b.data.into_iter().map(|d| String::from_utf8(d.data).unwrap()).collect();
            let next: Vec<String> = b.next_choices.iter().map(hex::encode).collect();

            blocks.push(BlockData {
                hash: hex::encode(b.hash),
                data,
                author: hex::encode(b.author),
                next_authors: next,
            });
        }
    }

    Json(blocks)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let app = App {
        peers: IndexMap::new(),
        flakies: IndexMap::new(),
        port_pool: 29102,
        site: site::IndexSite {
            peers: Vec::new(),
            flakies: Vec::new(),
            spawn_count: 1,
            is_flaking: false,
            flake_time: 1000,
            target: None,
            blocks: Vec::new(),
        },
    };

    let router = Router::new()
        .route("/", get(index))
        .route("/create", post(peer_create))
        .route("/select/{pubhex}", post(peer_select))
        .route("/circle-connect", post(circle_connect))
        .route("/shutdown/{pubhex}", post(peer_shutdown))
        .route("/share", post(peer_share))
        .route("/connect", post(peer_connect))
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::new(Mutex::new(app)));

    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .unwrap();
}
