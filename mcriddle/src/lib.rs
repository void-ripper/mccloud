use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Weak,
    },
    time::{Duration, SystemTime},
};

use blockchain::{Block, BlockIterator, Blockchain};
use client::Client;
pub use error::{Error, Result};
use indexmap::{IndexMap, IndexSet};
use k256::{
    ecdsa::{
        signature::{Signer, Verifier},
        Signature, SigningKey, VerifyingKey,
    },
    elliptic_curve::{rand_core::OsRng, sec1::ToEncodedPoint},
    SecretKey,
};
use message::Message;
use tokio::{
    net::{TcpListener, TcpStream},
    select,
    sync::{broadcast, mpsc, Mutex},
    time::{self},
};

mod blockchain;
mod client;
mod error;
mod message;

#[macro_export]
macro_rules! ex {
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
    pub keep_alive: Duration,
    pub data_gather_time: Duration,
    pub thin: bool,
}

type Clients = HashMap<PubKeyBytes, Arc<Mutex<Client>>>;

pub struct Peer {
    me: Weak<Peer>,
    pub cfg: Config,
    prikey: SecretKey,
    pubkey: PubKeyBytes,
    pubhex: String,
    last_block_tx: broadcast::Sender<Block>,
    to_handle_tx: mpsc::Sender<(Message, Arc<Mutex<Client>>)>,
    to_shutdown: Arc<AtomicBool>,
    clients: Mutex<Clients>,
    known: Mutex<IndexMap<PubKeyBytes, SystemTime>>,
    blockchain: Mutex<Blockchain>,
}

impl Peer {
    pub fn new(cfg: Config) -> Result<Arc<Self>> {
        let prikey = k256::SecretKey::random(&mut OsRng);
        let mut pubkey: PubKeyBytes = [0u8; 33];
        pubkey.copy_from_slice(prikey.public_key().to_encoded_point(true).as_bytes());
        let to_shutdown = Arc::new(AtomicBool::new(false));

        let blockchain = ex!(Blockchain::new(&cfg.folder), source);
        let (mtx, mrx) = mpsc::channel(10);

        let pubhex: String = hex::encode(&pubkey);

        tracing::info!(
            "root block {}",
            blockchain
                .root
                .as_ref()
                .map(|r| hex::encode(r))
                .unwrap_or(String::new())
        );
        tracing::info!(
            "last block {} {}",
            blockchain.count,
            blockchain
                .last
                .as_ref()
                .map(|r| hex::encode(r))
                .unwrap_or(String::new())
        );
        let (last_block_tx, _) = broadcast::channel(10);
        let peer = Arc::new_cyclic(|me| Self {
            // let peer = Arc::new(Self {
            me: me.clone(),
            prikey,
            pubkey,
            pubhex,
            cfg,
            last_block_tx,
            to_handle_tx: mtx,
            to_shutdown,
            clients: Mutex::new(HashMap::new()),
            known: Mutex::new(IndexMap::new()),
            blockchain: Mutex::new(blockchain),
        });

        let p0 = peer.clone();
        tokio::spawn(async move {
            if let Err(e) = p0.listen(mrx).await {
                tracing::error!("{} Loop: {e}", p0.pubhex);
            }
        });

        if !peer.cfg.thin {
            let p1 = peer.clone();
            tokio::spawn(async move {
                let mut interval = time::interval(p1.cfg.keep_alive);
                let signer = SigningKey::from(&p1.prikey);
                let sign: Signature = signer.sign(&p1.pubkey);
                let mut sign_bytes = [0u8; 64];
                sign_bytes.copy_from_slice(&sign.to_vec());

                while !p1.to_shutdown.load(Ordering::SeqCst) {
                    interval.tick().await;

                    if let Err(e) = p1
                        .broadcast(Message::KeepAlive {
                            pubkey: p1.pubkey,
                            sign: sign_bytes,
                        })
                        .await
                    {
                        tracing::error!("{} {}", p1.pubhex, e);
                    }
                }
            });
        }

        Ok(peer)
    }

    async fn broadcast_except(&self, msg: Message, except: &Arc<Mutex<Client>>) -> Result<()> {
        let except = except.lock().await.pubkey.clone();
        let cls = self.clients.lock().await;
        for to in cls.values() {
            let mut to = to.lock().await;
            if to.pubkey != except {
                ex!(to.write(&msg).await, source);
            }
        }

        Ok(())
    }

    async fn broadcast(&self, msg: Message) -> Result<()> {
        let cls = self.clients.lock().await;
        for to in cls.values() {
            let mut to = to.lock().await;
            ex!(to.write(&msg).await, source);
        }

        Ok(())
    }

    async fn listen(&self, mut to_handle: mpsc::Receiver<(Message, Arc<Mutex<Client>>)>) -> Result<()> {
        tracing::info!("{} start", self.pubhex);

        let listener = ex!(TcpListener::bind(self.cfg.addr.clone()).await, io);

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
                        if let Err(e) = self.on_message(msg,cl ).await {
                            tracing::error!("{} {}", self.pubhex, e);
                        }
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
            tx_nonce: 0,
        };

        let greeting = {
            let blkch = self.blockchain.lock().await;
            Message::Greeting {
                pubkey: self.pubkey.clone(),
                root: blkch.root.clone(),
                last: blkch.last.clone(),
                count: blkch.count,
                thin: self.cfg.thin,
            }
        };

        ex!(cl.write(&greeting).await, source);

        let (nonce, greeting) = ex!(Client::read(&mut reader, &cl.shared).await, source);

        if let Message::Greeting {
            pubkey,
            root,
            last: _,
            count,
            thin,
        } = greeting
        {
            tracing::info!(
                "{} greeting {} {} {}",
                self.pubhex,
                hex::encode(&pubkey),
                root.as_ref().map(|r| hex::encode(r)).unwrap_or("".into()),
                count
            );
            cl.pubkey = pubkey.clone();
            cl.shared_secret(&self.prikey);

            if !thin {
                self.known.lock().await.insert(pubkey, SystemTime::now());
            }

            let mut blkch = self.blockchain.lock().await;

            if !self.cfg.thin && !thin && root.is_none() && blkch.root.is_none() {
                if self.pubkey > pubkey {
                    tracing::info!("{} create genesis block", self.pubhex);
                    blkch.cache.insert(b"[genesis]".to_vec());
                    let blk = ex!(self.create_next_block(&mut *blkch).await, source);
                    ex!(cl.write(&Message::ShareBlock { block: blk }).await, source);
                }
            } else if blkch.root.is_none() {
                ex!(cl.write(&Message::RequestBlocks { start: None }).await, source);
            } else if count > blkch.count {
                ex!(
                    cl.write(&Message::RequestBlocks {
                        start: blkch.last.clone()
                    })
                    .await,
                    source
                );
            }
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

        let cl0 = cl.clone();
        self.clients.lock().await.insert(pubkey, cl0);

        let peer = self.me.upgrade().unwrap();

        tokio::spawn(async move {
            let mut rx_nonce = nonce;
            loop {
                let msg = Client::read(&mut reader, &shared).await;
                match msg {
                    Ok((nonce, msg)) => {
                        if nonce > rx_nonce {
                            rx_nonce = nonce;
                            if let Err(e) = to_handle.send((msg, cl.clone())).await {
                                tracing::error!("{} {}", pubhex, e);
                            }
                        } else {
                            tracing::warn!("{} nonce to low, omit message", pubhex);
                        }
                    }
                    Err(e) => {
                        tracing::error!("{} {}", pubhex, e);
                        break;
                    }
                }
            }

            peer.clients.lock().await.remove(&pubkey);
        });

        Ok(())
    }

    async fn create_next_block(&self, blkch: &mut Blockchain) -> Result<Block> {
        tracing::info!("{} create next block", self.pubhex);

        let next_author = {
            let k = self.known.lock().await;
            k.get_index(rand::random::<usize>() % k.len())
                .map(|(k, _v)| k.clone())
                .unwrap()
        };
        let blk = blkch.create_block(next_author, self.pubkey.clone(), &self.prikey);
        ex!(blkch.add_block(blk.clone()), source);

        Ok(blk)
    }

    async fn on_message(&self, msg: Message, cl: Arc<Mutex<Client>>) -> Result<()> {
        match msg {
            Message::Greeting { .. } => {
                tracing::error!("{} we should never get a second greeting", self.pubhex);
            }
            Message::KeepAlive { pubkey, sign } => {
                if pubkey == self.pubkey {
                    return Ok(());
                }
                let verifier = ex!(VerifyingKey::from_sec1_bytes(&pubkey), encrypt);
                let signature = ex!(Signature::from_slice(&sign), encrypt);
                ex!(verifier.verify(&pubkey, &signature), encrypt);

                if let Some(old) = self.known.lock().await.insert(pubkey.clone(), SystemTime::now()) {
                    // tracing::debug!("{} keep alive {}", self.pubhex, hex::encode(&pubkey));
                    let elapsed = old.elapsed().unwrap();
                    let delta = if elapsed > self.cfg.keep_alive {
                        0
                    } else {
                        (self.cfg.keep_alive - elapsed).as_millis()
                    };

                    if delta < 50 {
                        let msg = Message::KeepAlive { pubkey, sign };
                        ex!(self.broadcast_except(msg, &cl).await, source);
                    }
                }
            }
            Message::ShareData { data } => {
                tracing::info!("{} got data", self.pubhex);

                let unknown = self.blockchain.lock().await.cache.insert(data.clone());
                if unknown {
                    let msg = Message::ShareData { data };
                    ex!(self.broadcast_except(msg, &cl).await, source);
                }
            }
            Message::RequestBlocks { start } => {
                let blk_it = self.blockchain.lock().await.get_blocks(start);
                let mut cl = cl.lock().await;
                for block in blk_it {
                    let block = ex!(block, source);
                    ex!(cl.write(&Message::RequestedBlock { block }).await, source);
                }
            }
            Message::RequestedBlock { block } => {
                tracing::info!("{} got block {}", self.pubhex, hex::encode(&block.hash));

                ex!(self.blockchain.lock().await.add_block(block.clone()), source);
                ex!(self.last_block_tx.send(block), sync);
            }
            Message::ShareBlock { block } => {
                tracing::info!("{} share block {}", self.pubhex, hex::encode(&block.hash));

                if self.pubkey == block.next_choice {
                    tracing::info!("{} me is next xD", self.pubhex);

                    let peer = self.me.upgrade().unwrap();
                    tokio::spawn(async move {
                        let res: Result<()> = async {
                            loop {
                                tokio::time::sleep(peer.cfg.data_gather_time).await;

                                let mut blkch = peer.blockchain.lock().await;
                                if blkch.cache.len() > 0 {
                                    let block = ex!(peer.create_next_block(&mut *blkch).await, source);
                                    ex!(
                                        peer.broadcast(Message::ShareBlock { block: block.clone() }).await,
                                        source
                                    );
                                    ex!(peer.last_block_tx.send(block), sync);
                                    break;
                                }
                            }

                            Ok(())
                        }
                        .await;

                        if let Err(e) = res {
                            tracing::error!("{} {}", peer.pubhex, e);
                        }
                    });
                }
                ex!(self.blockchain.lock().await.add_block(block.clone()), source);

                ex!(
                    self.broadcast_except(Message::ShareBlock { block: block.clone() }, &cl)
                        .await,
                    source
                );
                ex!(self.last_block_tx.send(block), sync);
            }
        }

        Ok(())
    }

    pub fn pubhex(&self) -> String {
        self.pubhex.clone()
    }

    pub async fn client_pubkeys(&self) -> HashSet<PubKeyBytes> {
        let cl = self.clients.lock().await;
        cl.keys().cloned().collect()
    }

    pub async fn known_pubkeys(&self) -> IndexSet<PubKeyBytes> {
        self.known.lock().await.keys().cloned().collect()
    }

    pub async fn connect(&self, addr: SocketAddr) -> Result<()> {
        tracing::info!("{} connect to {}", self.pubhex, addr);
        let sck = ex!(TcpStream::connect(addr).await, io);
        self.accept(addr, sck).await?;

        Ok(())
    }

    pub async fn share(&self, data: Vec<u8>) -> Result<()> {
        let unknown = self.blockchain.lock().await.cache.insert(data.clone());
        if unknown {
            let msg = Message::ShareData { data };
            ex!(self.broadcast(msg).await, source);
        }

        Ok(())
    }

    pub fn last_block_receiver(&self) -> broadcast::Receiver<Block> {
        self.last_block_tx.subscribe()
    }

    pub async fn block_iter(&self) -> BlockIterator {
        self.blockchain.lock().await.get_blocks(None)
    }

    pub fn shutdown(&self) {
        self.to_shutdown.store(true, Ordering::SeqCst);
    }
}
