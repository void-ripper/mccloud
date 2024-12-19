#![doc = include_str!("../../README.md")]

use std::{
    future::Future,
    net::SocketAddr,
    path::PathBuf,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Weak,
    },
    time::{Duration, SystemTime},
};

use blockchain::{Block, BlockIterator, Blockchain, Data};
use client::{ClientInfo, ClientWriter};
use error::ErrorKind;
pub use error::{Error, Result};
use hashbrown::{hash_map::Entry, HashMap, HashSet};
use k256::{
    elliptic_curve::{rand_core::OsRng, sec1::ToEncodedPoint},
    SecretKey,
};
use message::Message;
use tokio::{
    net::{TcpListener, TcpStream},
    select,
    sync::{broadcast, mpsc, Mutex},
    time,
};
pub use version::Version;

pub mod blockchain;
mod client;
pub mod error;
mod message;
mod version;

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
pub struct ConfigRelationship {
    /// How many connections a node should have.
    pub count: u32,
    /// In which time intervals to look for new connections.
    pub time: Duration,
    /// How often to retry, after an already established connection is lost.
    pub retry: u32,
}

#[derive(Clone)]
pub struct Config {
    /// The address the peer is listening on.
    pub addr: SocketAddr,
    /// The data folder where to save the blockchain.
    pub folder: PathBuf,
    /// The time between keep alive updates.
    pub keep_alive: Duration,
    /// How long to gather new data until new block is generated.
    pub data_gather_time: Duration,
    /// A thin node does not participate in generating new blocks.
    pub thin: bool,
    /// The relationship config to other nodes.
    pub relationship: ConfigRelationship,
    /// How many candidates are allowed for the next block.
    pub next_candidates: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: ([0, 0, 0, 0], 29092).into(),
            folder: "data".into(),
            keep_alive: Duration::from_millis(600),
            data_gather_time: Duration::from_millis(750),
            thin: false,
            relationship: ConfigRelationship {
                time: Duration::from_secs(10),
                count: 3,
                retry: 3,
            },
            next_candidates: 3,
        }
    }
}

type Clients = HashMap<PubKeyBytes, Arc<ClientInfo>>;
type OnCreateCb = dyn Fn(HashMap<SignBytes, Data>) -> Pin<Box<dyn Future<Output = Result<HashMap<SignBytes, Data>>> + Send>>
    + Send
    + 'static;

pub struct Peer {
    me: Weak<Peer>,
    pub cfg: Config,
    version: Version,
    prikey: SecretKey,
    pubkey: PubKeyBytes,
    pubhex: String,
    to_accept: mpsc::Sender<(SocketAddr, u32)>,
    last_block_tx: broadcast::Sender<Block>,
    to_shutdown: broadcast::Sender<bool>,
    clients: Mutex<Clients>,
    known: Mutex<HashMap<PubKeyBytes, SystemTime>>,
    blockchain: Mutex<Blockchain>,
    on_block_creation: Mutex<Option<Box<OnCreateCb>>>,
    is_block_gathering: AtomicBool,
}

impl Peer {
    pub fn new(cfg: Config) -> Result<Arc<Self>> {
        let prikey = k256::SecretKey::random(&mut OsRng);
        let mut pubkey: PubKeyBytes = [0u8; 33];
        pubkey.copy_from_slice(prikey.public_key().to_encoded_point(true).as_bytes());
        let (to_shutdown, _) = broadcast::channel(1);

        let blockchain = ex!(Blockchain::new(&cfg.folder), source);

        let pubhex: String = hex::encode(pubkey);
        let version = Version::default();

        tracing::info!("{version}");
        tracing::info!(
            "root block {}",
            blockchain.root.as_ref().map(hex::encode).unwrap_or_default()
        );
        tracing::info!(
            "last block {} {}",
            blockchain.count,
            blockchain.last.as_ref().map(hex::encode).unwrap_or_default()
        );
        let (last_block_tx, _) = broadcast::channel(10);
        let (to_accept_tx, to_accept_rx) = mpsc::channel(10);

        let peer = Arc::new_cyclic(|me| Self {
            // let peer = Arc::new(Self {
            me: me.clone(),
            version,
            prikey,
            pubkey,
            pubhex,
            cfg,
            to_accept: to_accept_tx,
            last_block_tx,
            to_shutdown,
            clients: Mutex::new(HashMap::new()),
            known: Mutex::new(HashMap::new()),
            blockchain: Mutex::new(blockchain),
            on_block_creation: Mutex::new(None),
            is_block_gathering: AtomicBool::new(false),
        });

        let p0 = peer.clone();
        tokio::spawn(async move {
            if let Err(e) = p0.listen(to_accept_rx).await {
                tracing::error!("{} Loop: {e}", p0.pubhex);
            }
        });

        if !peer.cfg.thin {
            let p1 = peer.clone();
            tokio::spawn(async move {
                if let Err(e) = p1.send_and_check_keepalive().await {
                    tracing::error!("{} keepalive: {}", p1.pubhex, e);
                }
            });

            let p2 = peer.clone();
            tokio::spawn(async move {
                if let Err(e) = p2.establish_relationship().await {
                    tracing::error!("{} {}", p2.pubhex, e);
                }
            });

            let p3 = peer.clone();
            tokio::spawn(async move {
                p3.check_for_block_gathering().await;
            });
        }

        Ok(peer)
    }

    async fn send_and_check_keepalive(&self) -> Result<()> {
        let mut interval = time::interval(self.cfg.keep_alive);
        let msg = Message::keepalive(&self.pubkey, &self.prikey);
        let mut rx_shutdown = self.to_shutdown.subscribe();

        loop {
            select! {
                _ = rx_shutdown.recv() => { break; }
                _ = interval.tick() => {
                    if let Err(e) = self.broadcast(msg.clone()).await {
                        tracing::error!("{} {}", self.pubhex, e);
                    }

                    if !self.is_block_gathering.load(Ordering::SeqCst) && self.check_is_me_next().await {
                        self.start_block_gathering();
                    }

                    self.known.lock().await.retain(|_k, v| {
                        if let Ok(elapsed) = v.elapsed() {
                            elapsed < self.cfg.keep_alive
                        } else {
                            false
                        }
                    });
                }
            }
        }
        Ok(())
    }

    async fn check_for_block_gathering(&self) {
        let mut rx_shutdown = self.to_shutdown.subscribe();

        loop {
            select! {
                _ = rx_shutdown.recv() => { break; }
                _ = time::sleep(self.cfg.keep_alive) => {
                    if !self.is_block_gathering.load(Ordering::SeqCst) && self.check_is_me_next().await {
                        self.start_block_gathering();
                    }
                }
            }
        }
    }

    async fn establish_relationship(&self) -> Result<()> {
        let mut rx_shutdown = self.to_shutdown.subscribe();
        let mut interval = time::interval(self.cfg.relationship.time);

        loop {
            select! {
                _ = rx_shutdown.recv() => { break; }
                _ = interval.tick() => {
                    let current = self.clients.lock().await.len();

                    if current < self.cfg.relationship.count as _ {
                        let keys: Vec<PubKeyBytes> = self.clients.lock().await.keys().cloned().collect();
                        ex!(
                            self.broadcast(Message::RequestNeighbours {
                                count: self.cfg.relationship.count,
                                exclude: keys,
                            })
                            .await,
                            source
                        );
                    }
                }
            }
        }

        Ok(())
    }

    async fn broadcast_except(&self, msg: Message, except: &Arc<ClientInfo>) -> Result<()> {
        let except = except.pubkey;
        let cls = self.clients.lock().await;
        for to in cls.values() {
            if to.pubkey != except {
                ex!(to.write(&msg).await, source);
            }
        }

        Ok(())
    }

    async fn broadcast(&self, msg: Message) -> Result<()> {
        let cls = self.clients.lock().await;
        for to in cls.values() {
            ex!(to.write(&msg).await, source);
        }

        Ok(())
    }

    async fn listen(&self, mut to_accept: mpsc::Receiver<(SocketAddr, u32)>) -> Result<()> {
        tracing::info!("{} listen on {}", self.pubhex, self.cfg.addr);

        let listener = ex!(TcpListener::bind(self.cfg.addr).await, io);
        let mut rx_shutdown = self.to_shutdown.subscribe();

        loop {
            select! {
                _ = rx_shutdown.recv() => { break; }
                addr = to_accept.recv() => {
                    if let Some((addr, reconn_cnt)) = addr {
                        match TcpStream::connect(addr).await {
                            Ok(sck) => {
                                if let Err(e) = self.accept(addr, sck, reconn_cnt).await {
                                    tracing::error!("{} {}", self.pubhex, e);
                                }
                            }
                            Err(e) => {
                                tracing::error!("{} {}", self.pubhex, e);
                            }
                        }
                    }
                }
                res = listener.accept() => {
                    match res {
                        Ok((sck, addr)) => {
                            if let Err(e) = self.accept(addr, sck, 0).await {
                                tracing::error!("{} {}", self.pubhex, e);
                            }
                        }
                        Err(e) => {
                            tracing::error!("{} {}", self.pubhex, e);
                        }
                    }
                }
            }
        }

        tracing::info!("{} shutdown", self.pubhex);

        Ok(())
    }

    async fn accept(&self, addr: SocketAddr, sck: TcpStream, reconn_cnt: u32) -> Result<()> {
        tracing::info!("{} accept {}", self.pubhex, addr);

        let (mut reader, writer) = sck.into_split();
        let mut clw = ClientWriter {
            shared: None,
            sck: writer,
            tx_nonce: 0,
        };

        let (myroot, mylast, mycount, greeting) = {
            let blkch = self.blockchain.lock().await;
            (
                blkch.root,
                blkch.last,
                blkch.count,
                Message::Greeting {
                    version: self.version.clone(),
                    pubkey: self.pubkey,
                    listen: self.cfg.addr,
                    root: blkch.root,
                    last: blkch.last,
                    count: blkch.count,
                    thin: self.cfg.thin,
                },
            )
        };

        ex!(clw.write(&greeting).await, source);

        let (nonce, greeting) = ex!(ClientWriter::read(&mut reader, &None).await, source);

        if let Message::Greeting {
            version,
            pubkey,
            listen,
            root,
            last,
            count,
            thin,
        } = greeting
        {
            if self.version != version {
                return Err(Error::protocol(
                    line!(),
                    module_path!(),
                    "mcriddle versions do not match",
                ));
            }

            if myroot.is_some() && root.is_some() && myroot != root {
                return Err(Error::protocol(
                    line!(),
                    module_path!(),
                    "blockchain root does not match",
                ));
            }

            tracing::info!(
                "{} greeting\n{}\n{}\n{} {}",
                self.pubhex,
                hex::encode(pubkey),
                version,
                root.as_ref().map(hex::encode).unwrap_or("".into()),
                count,
            );
            let shared = clw.shared_secret(&pubkey, &self.prikey);
            let cl = ClientInfo {
                addr,
                listen,
                pubkey,
                writer: Mutex::new(clw),
            };

            if !thin {
                self.known.lock().await.insert(pubkey, SystemTime::now());
            }

            if (myroot.is_none() || count > mycount) && last.is_some() {
                ex!(cl.write(&Message::RequestBlocks { start: mylast }).await, source);
            }

            let pubkey = cl.pubkey;
            let pubhex = self.pubhex.clone();
            let cl = Arc::new(cl);

            let cl0 = cl.clone();
            self.clients.lock().await.insert(pubkey, cl0);

            let peer = self.me.upgrade().unwrap();
            let mut rx_shutdown = self.to_shutdown.subscribe();

            tokio::spawn(async move {
                let mut rx_nonce = nonce;
                loop {
                    select! {
                        _ = rx_shutdown.recv() => {
                            peer.clients.lock().await.remove(&pubkey);
                            return;
                        }
                        msg = ClientWriter::read(&mut reader, &shared) => {
                            match msg {
                                Ok((nonce, msg)) => {
                                    if nonce > rx_nonce {
                                        rx_nonce = nonce;
                                        if let Err(e) = peer.on_message(msg, cl.clone()).await {
                                            tracing::error!("{} {}", pubhex, e);
                                        }
                                    } else {
                                        tracing::warn!("{} nonce to low, omit message", pubhex);
                                    }
                                }
                                Err(e) => {
                                    if e.kind != ErrorKind::Disconnect {
                                        tracing::error!("{} {}", pubhex, e);
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }

                peer.clients.lock().await.remove(&pubkey);
                // peer.known.lock().await.swap_remove(&pubkey);

                if reconn_cnt > 0 {
                    tokio::time::sleep(Duration::from_secs(15)).await;
                    if !peer.to_accept.is_closed() {
                        if let Err(e) = peer.to_accept.send((addr, reconn_cnt - 1)).await {
                            tracing::error!("{} {}", peer.pubhex, e);
                        }
                    }
                }
            });
        } else {
            return Err(Error::protocol(
                line!(),
                module_path!(),
                "first message was not greeting",
            ));
        }

        Ok(())
    }

    async fn create_next_block(&self, blkch: &mut Blockchain) -> Result<Block> {
        tracing::info!("{} create next block", self.pubhex);

        if let Some(oncb) = &mut *self.on_block_creation.lock().await {
            let cache = blkch.cache.drain().collect();
            blkch.cache = ex!(oncb(cache).await, source);
        }

        let next_author = {
            let mut nexts = Vec::new();
            let mut k: Vec<PubKeyBytes> = self.known.lock().await.keys().cloned().collect();
            while !k.is_empty() && nexts.len() < self.cfg.next_candidates as _ {
                nexts.push(k.swap_remove(rand::random::<usize>() % k.len()));
            }
            nexts
        };
        let blk = ex!(blkch.create_block(next_author, self.pubkey, &self.prikey), source);
        ex!(blkch.add_block(blk.clone()), source);

        Ok(blk)
    }

    async fn check_is_me_next(&self) -> bool {
        let blkch = self.blockchain.lock().await;
        let known = self.known.lock().await;

        for nxt in blkch.next_authors.iter() {
            if known.contains_key(nxt) {
                return false;
            } else if *nxt == self.pubkey {
                return true;
            }
        }

        if !known.is_empty() && !blkch.cache.is_empty() {
            let mut known: Vec<PubKeyBytes> = known.keys().cloned().collect();
            known.push(self.pubkey);
            known.sort_unstable();

            if known[0] == self.pubkey {
                return true;
            }
        }

        false
    }

    fn start_block_gathering(&self) {
        if !self.is_block_gathering.swap(true, Ordering::SeqCst) {
            let peer = self.me.upgrade().unwrap();
            tokio::spawn(async move {
                tracing::info!("{} me is next xD", peer.pubhex);
                let mut to_shutdown = peer.to_shutdown.subscribe();

                let res: Result<()> = async {
                    loop {
                        select! {
                            _ = time::sleep(peer.cfg.data_gather_time) => {
                                let mut blkch = peer.blockchain.lock().await;
                                if !blkch.cache.is_empty() {
                                    let block = ex!(peer.create_next_block(&mut blkch).await, source);
                                    ex!(
                                        peer.broadcast(Message::ShareBlock { block: block.clone() }).await,
                                        source
                                    );
                                    if peer.last_block_tx.receiver_count() > 0 {
                                        ex!(peer.last_block_tx.send(block), sync);
                                    }
                                    break;
                                }
                            }
                            _ = to_shutdown.recv() => { break; }
                        }
                    }

                    peer.is_block_gathering.store(false, Ordering::SeqCst);

                    Ok(())
                }
                .await;

                if let Err(e) = res {
                    tracing::error!("{} {}", peer.pubhex, e);
                }
            });
        }
    }

    async fn on_message(&self, msg: Message, cl: Arc<ClientInfo>) -> Result<()> {
        match msg {
            Message::Greeting { .. } => {
                tracing::error!("{} we should never get a second greeting", self.pubhex);
            }
            m @ Message::KeepAlive { pubkey, .. } => {
                ex!(self.on_keepalive(pubkey, m, cl).await, source);
            }
            Message::ShareData { data } => {
                ex!(self.on_share_data(data, cl).await, source);
            }
            Message::RequestBlocks { start } => {
                ex!(self.on_request_blocks(start, cl).await, source);
            }
            Message::RequestedBlock { block } => {
                ex!(self.on_requested_block(block).await, source);
            }
            Message::ShareBlock { block } => {
                ex!(self.on_share_block(block, cl).await, source);
            }
            Message::RequestNeighbours { count, exclude } => {
                ex!(self.on_request_neighbours(count, exclude, cl).await, source);
            }
            Message::IntroduceNeighbours { neighbours } => {
                ex!(self.on_introduce_neighbours(neighbours).await, source);
            }
        }

        Ok(())
    }

    async fn on_keepalive(&self, pubkey: PubKeyBytes, m: Message, cl: Arc<ClientInfo>) -> Result<()> {
        if pubkey == self.pubkey {
            return Ok(());
        }

        // if ex!(m.verify(), source) {
        let previous = self.known.lock().await.insert(pubkey, SystemTime::now());

        if let Some(old) = previous {
            // tracing::debug!("{} keep alive {}", self.pubhex, hex::encode(&pubkey));
            let elapsed = old.elapsed().unwrap_or(self.cfg.keep_alive);
            let delta = if elapsed >= self.cfg.keep_alive {
                0
            } else {
                (self.cfg.keep_alive - elapsed).as_millis()
            };

            if delta < 50 {
                ex!(self.broadcast_except(m, &cl).await, source);
            }
        } else if previous.is_none() {
            ex!(self.broadcast_except(m, &cl).await, source);
        }
        // }

        Ok(())
    }

    async fn perform_share(&self, data: Data, cl: Option<Arc<ClientInfo>>) -> Result<()> {
        let mut blkch = self.blockchain.lock().await;

        if let Entry::Vacant(e) = blkch.cache.entry(data.sign) {
            e.insert(data.clone());
            let msg = Message::ShareData { data };

            if let Some(cl) = cl {
                ex!(self.broadcast_except(msg, &cl).await, source);
            } else {
                ex!(self.broadcast(msg).await, source);
            }
        }

        Ok(())
    }

    async fn on_share_data(&self, data: Data, cl: Arc<ClientInfo>) -> Result<()> {
        tracing::info!("{} got data", self.pubhex);

        self.perform_share(data, Some(cl)).await
    }

    async fn on_request_blocks(&self, start: Option<HashBytes>, cl: Arc<ClientInfo>) -> Result<()> {
        tracing::info!(
            "{} request for blocks {}",
            self.pubhex,
            start.map(hex::encode).unwrap_or_default()
        );
        let blk_it = self.blockchain.lock().await.get_blocks(start);
        for block in blk_it {
            let block = ex!(block, source);
            ex!(cl.write(&Message::RequestedBlock { block }).await, source);
        }

        Ok(())
    }

    async fn on_requested_block(&self, block: Block) -> Result<()> {
        tracing::info!("{} got block {}", self.pubhex, hex::encode(block.hash));

        ex!(self.blockchain.lock().await.add_block(block.clone()), source);
        if self.last_block_tx.receiver_count() > 0 {
            ex!(self.last_block_tx.send(block), sync);
        }

        Ok(())
    }

    async fn on_share_block(&self, block: Block, cl: Arc<ClientInfo>) -> Result<()> {
        {
            let mut blkch = self.blockchain.lock().await;
            let last = blkch.last;
            if last.map(|n| n == block.hash).unwrap_or(false) {
                // we get the same block again, just ignore it
                return Ok(());
            }

            tracing::info!("{} share block {}", self.pubhex, hex::encode(block.hash));

            ex!(blkch.add_block(block.clone()), source);
        }

        ex!(
            self.broadcast_except(Message::ShareBlock { block: block.clone() }, &cl)
                .await,
            source
        );

        if self.last_block_tx.receiver_count() > 0 {
            ex!(self.last_block_tx.send(block), sync);
        }

        Ok(())
    }

    async fn on_request_neighbours(&self, count: u32, exclude: Vec<PubKeyBytes>, cl: Arc<ClientInfo>) -> Result<()> {
        let possible: Vec<(PubKeyBytes, SocketAddr)> = {
            let cls = self.clients.lock().await;
            cls.iter().map(|(k, v)| (*k, v.listen)).collect()
        };
        let mut exclude: HashSet<PubKeyBytes> = exclude.into_iter().collect();

        exclude.insert(cl.pubkey);

        let mut possible: Vec<(PubKeyBytes, SocketAddr)> =
            possible.into_iter().filter(|(k, _)| !exclude.contains(k)).collect();

        // let to_exclude: Vec<String> = exclude.iter().map(|x| hex::encode(x)).collect();
        // let to_exclude = to_exclude.join("\n");
        // tracing::info!(
        //     "{} request for neighbours\nfrom: {}\n{}",
        //     self.pubhex,
        //     hex::encode(cl.pubkey),
        //     to_exclude
        // );

        let mut to_share = Vec::new();
        if count < possible.len() as _ {
            while to_share.len() < count as _ {
                let m = possible.swap_remove(rand::random::<usize>() % possible.len());
                to_share.push(m);
            }
        } else {
            to_share.extend(possible.into_iter());
        }

        if !to_share.is_empty() {
            ex!(
                cl.write(&Message::IntroduceNeighbours { neighbours: to_share }).await,
                source
            );
        }

        Ok(())
    }

    async fn on_introduce_neighbours(&self, neighbours: Vec<(PubKeyBytes, SocketAddr)>) -> Result<()> {
        // let to_connect: Vec<String> = neighbours.iter().map(|x| hex::encode(x.0)).collect();
        // let to_connect = to_connect.join("\n");
        // tracing::info!(
        //     "{} introduce new neighbours {}\n{}",
        //     self.pubhex,
        //     neighbours.len(),
        //     to_connect
        // );
        let cnt = self.clients.lock().await.len();
        if cnt < self.cfg.relationship.count as _ {
            let to_add = self.cfg.relationship.count as usize - cnt;
            for (_k, n) in neighbours.into_iter().take(to_add) {
                ex!(self.to_accept.send((n, self.cfg.relationship.retry)).await, sync);
            }
        }

        Ok(())
    }

    /// Returns the hex representation of the public key.
    pub fn pubhex(&self) -> String {
        self.pubhex.clone()
    }

    /// Returns the public keys of the directly connected peers.
    pub async fn client_pubkeys(&self) -> HashSet<PubKeyBytes> {
        let cl = self.clients.lock().await;
        cl.keys().cloned().collect()
    }

    /// Returns all known public keys.
    pub async fn known_pubkeys(&self) -> HashSet<PubKeyBytes> {
        self.known.lock().await.keys().cloned().collect()
    }

    /// Sets a callback that is only called on the peer which creates block.
    ///
    /// This is usefull if you want to validate the data or want to perform a truly atomic action in the network.
    pub async fn set_on_block_creation_cb<F>(&self, cb: F)
    where
        F: Fn(HashMap<SignBytes, Data>) -> Pin<Box<dyn Future<Output = Result<HashMap<SignBytes, Data>>> + Send>>
            + Send
            + 'static,
    {
        *self.on_block_creation.lock().await = Some(Box::new(cb));
    }

    /// Try to connect to another peer.
    pub async fn connect(&self, addr: SocketAddr) -> Result<()> {
        tracing::info!("{} connect to {}", self.pubhex, addr);
        ex!(self.to_accept.send((addr, self.cfg.relationship.retry)).await, sync);
        Ok(())
    }

    pub fn create_data(&self, data: Vec<u8>) -> Result<Data> {
        let data = ex!(Data::new(data, &self.pubkey, &self.prikey), source);
        Ok(data)
    }

    /// Share data in the network.
    pub async fn share(&self, data: Vec<u8>) -> Result<()> {
        let data = ex!(Data::new(data, &self.pubkey, &self.prikey), source);
        self.perform_share(data, None).await
    }

    /// Returns a new tokio::broadcast::Receiver which gets the new last block.
    pub fn last_block_receiver(&self) -> broadcast::Receiver<Block> {
        self.last_block_tx.subscribe()
    }

    /// Returns an iterator over all blocks, from start to last.
    pub async fn block_iter(&self) -> BlockIterator {
        self.blockchain.lock().await.get_blocks(None)
    }

    pub fn shutdown(&self) -> Result<()> {
        ex!(self.to_shutdown.send(true), sync);
        Ok(())
    }
}
