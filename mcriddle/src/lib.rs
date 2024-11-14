use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use blockchain::{BlockIterator, Blockchain};
use client::Client;
use error::{Error, Result};
use k256::{
    elliptic_curve::{rand_core::OsRng, sec1::ToEncodedPoint},
    SecretKey,
};
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
pub type SignBytes = [u8; 64];

#[derive(Clone)]
pub struct Config {
    pub addr: String,
    pub folder: PathBuf,
}

type Clients = HashMap<PubKeyBytes, Arc<Mutex<Client>>>;

pub struct Peer {
    // me: Weak<Peer>,
    pub cfg: Config,
    prikey: SecretKey,
    pubkey: PubKeyBytes,
    pubhex: String,
    to_handle_tx: mpsc::Sender<(Message, Arc<Mutex<Client>>)>,
    to_shutdown: Arc<AtomicBool>,
    clients: Mutex<Clients>,
    known: Mutex<HashSet<PubKeyBytes>>,
    blockchain: Mutex<Blockchain>,
}

impl Peer {
    pub fn new(cfg: Config) -> Result<Arc<Self>> {
        let prikey = k256::SecretKey::random(&mut OsRng);
        let mut pubkey: PubKeyBytes = [0u8; 33];
        pubkey.copy_from_slice(prikey.public_key().to_encoded_point(true).as_bytes());
        let to_shutdown = Arc::new(AtomicBool::new(false));

        let blockchain = Blockchain::new(&cfg.folder);
        let (mtx, mrx) = mpsc::channel(10);

        let mut pubhex: String = hex::encode(&pubkey);
        pubhex.truncate(12);

        // let peer = Arc::new_cyclic(|me| Self {
        let peer = Arc::new(Self {
            // me: me.clone(),
            prikey,
            pubkey,
            pubhex,
            cfg,
            to_handle_tx: mtx,
            to_shutdown,
            clients: Mutex::new(HashMap::new()),
            known: Mutex::new(HashSet::new()),
            blockchain: Mutex::new(blockchain),
        });

        let p0 = peer.clone();
        tokio::spawn(async move {
            if let Err(e) = p0.listen(mrx).await {
                tracing::error!("{} Loop: {e}", p0.pubhex);
            }
        });

        Ok(peer)
    }

    async fn broadcast_except(&self, msg: Message, except: &Arc<Mutex<Client>>) -> Result<()> {
        let except = except.lock().await.pubkey.clone();
        let cls = self.clients.lock().await;
        for to in cls.values() {
            let mut to = to.lock().await;
            if to.pubkey != except {
                guard!(to.write(&msg).await, source);
            }
        }

        Ok(())
    }

    async fn broadcast(&self, msg: Message) -> Result<()> {
        let cls = self.clients.lock().await;
        for to in cls.values() {
            let mut to = to.lock().await;
            guard!(to.write(&msg).await, source);
        }

        Ok(())
    }

    async fn listen(&self, mut to_handle: mpsc::Receiver<(Message, Arc<Mutex<Client>>)>) -> Result<()> {
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
                msg = to_handle.recv() => {
                    if let Some((msg, cl)) = msg {
                        guard!(self.on_message(msg,cl ).await, source);
                    }
                }
            }
        }

        tracing::info!("{} shutdown", self.pubhex);

        Ok(())
    }

    async fn accept(&self, addr: SocketAddr, sck: TcpStream) -> Result<()> {
        tracing::info!("{} accept {}", self.pubhex, addr);

        let (mut reader, writer) = sck.into_split();
        let mut cl = Client {
            addr,
            sck: writer,
            pubkey: [0u8; 33],
            shared: None,
            nonce: 0,
        };

        let greeting = {
            let blkch = self.blockchain.lock().await;
            Message::Greeting {
                pubkey: self.pubkey.clone(),
                root: blkch.root.clone(),
                last: blkch.last.clone(),
                count: blkch.count,
            }
        };

        guard!(cl.write(&greeting).await, source);

        let greeting = guard!(Client::read(&mut reader, &cl.shared).await, source);

        if let Message::Greeting {
            pubkey,
            root,
            last,
            count,
        } = greeting
        {
            let mut blkch = self.blockchain.lock().await;

            if root.is_none() && blkch.root.is_none() {
                if self.pubkey > pubkey {
                    tracing::info!("{} create genesis block", self.pubhex);
                    let blk = blkch.create_block(self.pubkey.clone(), &self.prikey);
                    guard!(blkch.add_block(blk.clone()), source);

                    guard!(cl.write(&Message::ShareBlock { block: blk }).await, source);
                }
            } else if blkch.root.is_none() {
                guard!(cl.write(&Message::RequestBlocks { start: None }).await, source);
            } else if count > blkch.count {
                guard!(
                    cl.write(&Message::RequestBlocks {
                        start: blkch.last.clone()
                    })
                    .await,
                    source
                );
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

        if pubkey != self.pubkey && self.known.lock().await.insert(pubkey.clone()) {
            let msg = Message::Announce { pubkey };
            guard!(self.broadcast(msg).await, source);
        }

        let cl0 = cl.clone();
        self.clients.lock().await.insert(pubkey, cl0);

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

    async fn on_message(&self, msg: Message, cl: Arc<Mutex<Client>>) -> Result<()> {
        match msg {
            Message::Greeting { .. } => {
                tracing::error!("{} we should never get a second greeting", self.pubhex);
            }
            Message::Announce { pubkey } => {
                if pubkey != self.pubkey && self.known.lock().await.insert(pubkey.clone()) {
                    let msg = Message::Announce { pubkey };
                    guard!(self.broadcast_except(msg, &cl).await, source);
                }
            }
            Message::Remove { pubkey } => {
                let msg = Message::Remove { pubkey };
                guard!(self.broadcast_except(msg, &cl).await, source);
            }
            Message::ShareData { data } => {
                let unknown = self.blockchain.lock().await.cache.insert(data.clone());
                if unknown {
                    let msg = Message::ShareData { data };
                    guard!(self.broadcast_except(msg, &cl).await, source);
                }
            }
            Message::RequestBlocks { start } => {
                let blk_it = self.blockchain.lock().await.get_blocks(start);
                let mut cl = cl.lock().await;
                for block in blk_it {
                    guard!(cl.write(&Message::RequestedBlock { block }).await, source);
                }
            }
            Message::RequestedBlock { block } => {
                guard!(self.blockchain.lock().await.add_block(block), source);
            }
            Message::ShareBlock { block } => {
                guard!(self.blockchain.lock().await.add_block(block.clone()), source);

                guard!(self.broadcast_except(Message::ShareBlock { block }, &cl).await, source);
            }
        }

        Ok(())
    }

    pub fn pubhex(&self) -> String {
        self.pubhex.clone()
    }

    pub async fn connect(&self, addr: SocketAddr) -> Result<()> {
        tracing::info!("{} connect to {}", self.pubhex, addr);
        let sck = guard!(TcpStream::connect(addr).await, io);
        self.accept(addr, sck).await?;

        Ok(())
    }

    pub async fn share(&self, data: Vec<u8>) -> Result<()> {
        let unknown = self.blockchain.lock().await.cache.insert(data.clone());
        if unknown {
            let msg = Message::ShareData { data };
            guard!(self.broadcast(msg).await, source);
        }

        Ok(())
    }

    pub async fn block_it(&self) -> BlockIterator {
        self.blockchain.lock().await.get_blocks(None)
    }

    pub fn shutdown(&self) {
        self.to_shutdown.store(true, Ordering::SeqCst);
    }
}
