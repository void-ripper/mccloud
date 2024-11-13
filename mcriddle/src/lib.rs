use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use blockchain::Blockchain;
use client::Client;
use error::{Error, Result};
use k256::{elliptic_curve::sec1::ToEncodedPoint, SecretKey};
use message::Message;
use tokio::{
    net::{TcpListener, TcpStream},
    select,
    sync::{mpsc, Mutex},
};

mod blockchain;
mod client;
mod error;
mod message;

#[macro_export]
macro_rules! guard {
    ($e: expr, $name: tt) => {
        $e.map_err(|e| Error::$name(line!(), module_path!(), e))?
    };
}

pub type PubKeyBytes = [u8; 33];
pub type HashBytes = [u8; 32];

#[derive(Clone)]
pub struct Config {
    pub addr: String,
    pub folder: PathBuf,
}

type Clients = HashMap<PubKeyBytes, Arc<Mutex<Client>>>;

pub struct Listener {
    cfg: Config,
    prikey: SecretKey,
    pubkey: PubKeyBytes,
    pubhex: String,
    to_accept: mpsc::Receiver<SocketAddr>,
    to_share: mpsc::Receiver<Vec<u8>>,
    to_handle: mpsc::Receiver<(Message, Arc<Mutex<Client>>)>,
    to_handle_tx: mpsc::Sender<(Message, Arc<Mutex<Client>>)>,
    to_shutdown: Arc<AtomicBool>,
    clients: Clients,
    known: HashSet<PubKeyBytes>,
    blockchain: Blockchain,
}

impl Listener {
    pub fn new(
        cfg: Config,
        prikey: SecretKey,
        pubkey: PubKeyBytes,
        pubhex: String,
        to_accept: mpsc::Receiver<SocketAddr>,
        to_share: mpsc::Receiver<Vec<u8>>,
        to_shutdown: Arc<AtomicBool>,
    ) -> Self {
        let blockchain = Blockchain::new(&cfg.folder);
        let (mtx, mrx) = mpsc::channel(10);
        Self {
            cfg,
            prikey,
            pubkey,
            pubhex,
            to_accept,
            to_share,
            to_handle: mrx,
            to_handle_tx: mtx,
            to_shutdown,
            clients: HashMap::new(),
            known: HashSet::new(),
            blockchain,
        }
    }

    async fn broadcast_except(&mut self, msg: Message, except: &Arc<Mutex<Client>>) -> Result<()> {
        let except = except.lock().await.pubkey.clone();
        for to in self.clients.values() {
            let mut to = to.lock().await;
            if to.pubkey != except {
                guard!(to.write(&msg).await, source);
            }
        }

        Ok(())
    }

    async fn broadcast(&mut self, msg: Message) -> Result<()> {
        for to in self.clients.values() {
            let mut to = to.lock().await;
            guard!(to.write(&msg).await, source);
        }

        Ok(())
    }

    async fn listen(&mut self) -> Result<()> {
        tracing::info!("{} start", self.pubhex);

        let listener = guard!(TcpListener::bind(self.cfg.addr.clone()).await, io);

        while !self.to_shutdown.load(Ordering::SeqCst) {
            select! {
                res = listener.accept() => {
                    match res {
                        Ok((sck, addr)) => {
                            self.accept(addr, sck).await?;
                        }
                        Err(e) => {
                            tracing::error!("{} {}", self.pubhex, e);
                        }
                    }
                }
                addr = self.to_accept.recv() => {
                    if let Some(addr) = addr {
                        tracing::info!("{} connect to {}", self.pubhex, addr);
                        let sck = guard!(TcpStream::connect(addr).await, io);
                        self.accept(addr, sck).await?;
                    }
                }
                data = self.to_share.recv() => {
                    if let Some(data) = data {
                        if self.blockchain.cache.insert(data.clone()) {
                            let msg = Message::ShareData { data };
                            guard!(self.broadcast(msg).await, source);
                        }
                    }
                }
                msg = self.to_handle.recv() => {
                    if let Some((msg, cl)) = msg {
                        guard!(self.on_message(msg,cl ).await, source);
                    }
                }
            }
        }

        tracing::info!("{} shutdown", self.pubhex);

        Ok(())
    }

    async fn accept(&mut self, addr: SocketAddr, sck: TcpStream) -> Result<()> {
        tracing::info!("{} accept {}", self.pubhex, addr);

        let (mut reader, writer) = sck.into_split();
        let mut cl = Client {
            addr,
            sck: writer,
            pubkey: [0u8; 33],
            shared: None,
            nonce: 0,
        };

        let greeting = Message::Greeting {
            pubkey: self.pubkey.clone(),
            root: self.blockchain.root.clone(),
        };

        guard!(cl.write(&greeting).await, source);

        let greeting = guard!(Client::read(&mut reader, &cl.shared).await, source);

        if let Message::Greeting { pubkey, root } = greeting {
            if root.is_none() && self.blockchain.root.is_none() {
            } else if self.blockchain.root.is_none() {
                guard!(cl.write(&Message::RequestBlocks { start: None }).await, source);
            }
            cl.pubkey = pubkey.clone();
            cl.shared_secret(&self.prikey);
        } else {
            return Err(Error::protocol(
                line!(),
                module_path!(),
                "first message was not greeting",
            ));
        }

        let pubkey = cl.pubkey;
        let shared = cl.shared;
        let pubhex = self.pubhex.clone();
        let to_handle = self.to_handle_tx.clone();
        let cl = Arc::new(Mutex::new(cl));

        if pubkey != self.pubkey && self.known.insert(pubkey.clone()) {
            let msg = Message::Announce { pubkey };
            guard!(self.broadcast(msg).await, source);
        }

        let cl0 = cl.clone();
        self.clients.insert(pubkey, cl0);

        tokio::spawn(async move {
            loop {
                let msg = Client::read(&mut reader, &shared).await;
                match msg {
                    Ok(msg) => {
                        if let Err(e) = to_handle.send((msg, cl.clone())).await {
                            tracing::error!("{} {}", pubhex, e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("{} {}", pubhex, e);
                    }
                }
            }
        });

        Ok(())
    }

    async fn on_message(&mut self, msg: Message, cl: Arc<Mutex<Client>>) -> Result<()> {
        match msg {
            Message::Greeting { .. } => {
                tracing::error!("{} we should never get a second greeting", self.pubhex);
            }
            Message::Announce { pubkey } => {
                if pubkey != self.pubkey && self.known.insert(pubkey.clone()) {
                    let msg = Message::Announce { pubkey };
                    guard!(self.broadcast_except(msg, &cl).await, source);
                }
            }
            Message::Remove { pubkey } => {
                let msg = Message::Remove { pubkey };
                guard!(self.broadcast_except(msg, &cl).await, source);
            }
            Message::ShareData { data } => {
                if self.blockchain.cache.insert(data.clone()) {
                    let msg = Message::ShareData { data };
                    guard!(self.broadcast_except(msg, &cl).await, source);
                }
            }
            Message::RequestBlocks { start } => {
                let blk_it = self.blockchain.get_blocks(start);
                let mut cl = cl.lock().await;
                for block in blk_it {
                    guard!(cl.write(&Message::RequestedBlock { block }).await, source);
                }
            }
            Message::RequestedBlock { block } => {
                guard!(self.blockchain.add_block(block), source);
            }
            Message::ShareBlock { block } => {
                guard!(self.blockchain.add_block(block.clone()), source);

                guard!(self.broadcast_except(Message::ShareBlock { block }, &cl).await, source);
            }
        }

        Ok(())
    }
}

pub struct Peer {
    pub cfg: Config,
    prikey: SecretKey,
    pubkey: PubKeyBytes,
    to_shutdown: Arc<AtomicBool>,
    to_accept: mpsc::Sender<SocketAddr>,
    to_share: mpsc::Sender<Vec<u8>>,
}

impl Peer {
    pub fn new(cfg: Config) -> Result<Arc<Self>> {
        let prikey = k256::SecretKey::random(&mut rand::thread_rng());
        let mut pubkey: PubKeyBytes = [0u8; 33];
        pubkey.copy_from_slice(prikey.public_key().to_encoded_point(true).as_bytes());
        let (to_accept_tx, to_accept_rx) = mpsc::channel(3);
        let (to_share_tx, to_share_rx) = mpsc::channel(3);
        let to_shutdown = Arc::new(AtomicBool::new(false));

        let mut pubhex: String = hex::encode(&pubkey);
        pubhex.truncate(12);
        let cfg0 = cfg.clone();
        let prikey0 = prikey.clone();
        let pubkey0 = pubkey.clone();
        let pubhex0 = pubhex.clone();
        let to_shutdown0 = to_shutdown.clone();

        tokio::spawn(async move {
            let mut listener = Listener::new(cfg0, prikey0, pubkey0, pubhex0, to_accept_rx, to_share_rx, to_shutdown0);
            if let Err(e) = listener.listen().await {
                tracing::error!("{} Loop: {e}", pubhex);
            }
        });

        Ok(Arc::new(Self {
            prikey,
            pubkey,
            cfg,
            to_shutdown,
            to_accept: to_accept_tx,
            to_share: to_share_tx,
        }))
    }

    pub fn pubhex(&self) -> String {
        hex::encode(self.pubkey)
    }

    pub async fn connect(&self, addr: SocketAddr) {
        self.to_accept.send(addr).await.unwrap();
    }

    pub async fn share(&self, data: Vec<u8>) {
        self.to_share.send(data).await.unwrap();
    }

    pub fn shutdown(&self) {
        self.to_shutdown.store(true, Ordering::SeqCst);
    }
}
