#![doc = include_str!("../../README.md")]

use std::{
    fmt::Display,
    future::Future,
    net::{SocketAddr, ToSocketAddrs},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Weak,
    },
    time::SystemTime,
};

use blockchain::{Block, BlockIterator, Blockchain, Data};
use client::{ClientInfo, ClientReader, ClientWriter};
use config::{Algorithm, Config};
use error::ErrorKind;
pub use error::{Error, Result};
use hashbrown::{hash_map::Entry, HashMap, HashSet};
use k256::{
    elliptic_curve::{rand_core::OsRng, sec1::ToEncodedPoint},
    SecretKey,
};
use message::Message;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    select,
    sync::{broadcast, mpsc, Mutex, RwLock},
    time,
};
use tokio_socks::tcp::Socks5Stream;
pub use tokio_socks::{IntoTargetAddr, TargetAddr};
pub use version::Version;

pub mod blockchain;
mod client;
pub mod config;
pub mod error;
pub mod highlander;
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

type Clients = HashMap<PubKeyBytes, Arc<ClientInfo>>;
type OnCreateCb = dyn Fn(HashMap<SignBytes, Data>) -> Pin<Box<dyn Future<Output = Result<HashMap<SignBytes, Data>>> + Send>>
    + Send
    + 'static;

fn target_addr_to_string<'a>(b: TargetAddr<'a>) -> String {
    match b {
        TargetAddr::Ip(ip) => ip.to_string(),
        TargetAddr::Domain(d, p) => format!("{}:{}", d, p),
    }
}

pub struct Peer {
    me: Weak<Peer>,
    pub cfg: Config,
    version: Version,
    prikey: SecretKey,
    pubkey: PubKeyBytes,
    pubhex: String,
    to_accept: mpsc::Sender<(TargetAddr<'static>, u32, Option<PubKeyBytes>)>,
    last_block_tx: broadcast::Sender<Block>,
    to_shutdown: broadcast::Sender<bool>,
    clients: RwLock<Clients>,
    known: RwLock<HashSet<PubKeyBytes>>,
    blockchain: RwLock<Blockchain>,
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
            clients: RwLock::new(HashMap::new()),
            known: RwLock::new(HashSet::new()),
            blockchain: RwLock::new(blockchain),
            on_block_creation: Mutex::new(None),
            is_block_gathering: AtomicBool::new(false),
        });

        let p0 = peer.clone();
        tokio::spawn(async move {
            if let Err(e) = p0.listen(to_accept_rx).await {
                tracing::error!("{} listen: {e}", p0.pubhex);
            }
        });

        let p2 = peer.clone();
        tokio::spawn(async move {
            if let Err(e) = p2.establish_relationship().await {
                tracing::error!("{} relationship: {}", p2.pubhex, e);
            }
        });

        if !peer.cfg.thin {
            let p3 = peer.clone();
            tokio::spawn(async move {
                p3.check_for_block_gathering().await;
            });
        }

        Ok(peer)
    }

    async fn check_for_block_gathering(&self) {
        let mut rx_shutdown = self.to_shutdown.subscribe();

        loop {
            select! {
                _ = rx_shutdown.recv() => { break; }
                _ = time::sleep(self.cfg.data_gather_time) => {
                    if !self.is_block_gathering.load(Ordering::SeqCst) && self.check_is_me_next().await {
                        self.start_block_gathering();
                    }
                }
            }
        }

        tracing::debug!("{} stop check block gathering", self.pubhex);
    }

    async fn establish_relationship(&self) -> Result<()> {
        let mut rx_shutdown = self.to_shutdown.subscribe();
        let mut interval = time::interval(self.cfg.relationship.time);

        loop {
            select! {
                _ = rx_shutdown.recv() => { break; }
                _ = interval.tick() => {
                    let current = self.clients.read().await.len();

                    if current < self.cfg.relationship.count as _ {
                        let keys: Vec<PubKeyBytes> = self.clients.read().await.keys().cloned().collect();
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

        tracing::debug!("{} stop checking relationships", self.pubhex);

        Ok(())
    }

    async fn broadcast_except(&self, msg: Message, except: &Arc<ClientInfo>) -> Result<()> {
        let except = except.pubkey;
        let cls = self.clients.read().await;
        for to in cls.values() {
            if to.pubkey != except {
                ex!(to.write(&msg).await, source);
            }
        }

        Ok(())
    }

    async fn broadcast(&self, msg: Message) -> Result<()> {
        let cls = self.clients.read().await;
        for to in cls.values() {
            ex!(to.write(&msg).await, source);
        }

        Ok(())
    }

    async fn listen(
        &self,
        mut to_accept: mpsc::Receiver<(TargetAddr<'static>, u32, Option<PubKeyBytes>)>,
    ) -> Result<()> {
        tracing::info!("{} listen on {}", self.pubhex, self.cfg.addr);

        let listener = ex!(TcpListener::bind(self.cfg.addr).await, io);
        let mut rx_shutdown = self.to_shutdown.subscribe();

        async fn accepting<E: Display>(
            peer: &Peer,
            addr: TargetAddr<'static>,
            sck: std::result::Result<TcpStream, E>,
            reconn_cnt: u32,
            pubkey: Option<PubKeyBytes>,
        ) {
            match sck {
                Ok(sck) => {
                    if let Err(e) = peer.accept(addr, sck, reconn_cnt).await {
                        tracing::error!("{} while accepting: {}", peer.pubhex, e);
                    }
                }
                Err(e) => {
                    tracing::error!("{} while connecting: {}", peer.pubhex, e);

                    if let Some(pubkey) = pubkey {
                        peer.trigger_leave(pubkey, addr, reconn_cnt);
                    }
                }
            }
        }

        'main: loop {
            select! {
                _ = rx_shutdown.recv() => { break 'main; }
                addr = to_accept.recv() => {
                    if let Some((addr, reconn_cnt, pubkey)) = addr {
                        if let Some(proxy) = &self.cfg.proxy {
                            let res = Socks5Stream::connect(proxy.proxy, addr.to_owned()).await;
                            accepting(self, addr, res.map(|e| e.into_inner()), reconn_cnt, pubkey).await;
                        }
                        else {
                            let raddr = addr.to_socket_addrs()
                                .map_err(|e| Error::io(line!(), module_path!(), e))
                                .and_then(|mut x| x.next().ok_or( Error::external(line!(),module_path!() ,"no socket address".into())));
                            match raddr {
                                Ok(raddr) => {
                                    accepting(self, addr, TcpStream::connect(raddr).await, reconn_cnt, None).await;
                                }
                                Err(e) => {
                                    tracing::error!("{} {}", self.pubhex, e);
                                }
                            }
                        }
                    }
                }
                res = listener.accept() => {
                    match res {
                        Ok((sck, addr)) => {
                            accepting(self, TargetAddr::Ip(addr).to_owned(), Ok::<_, std::io::Error>(sck), 0, None).await;
                        }
                        Err(e) => {
                            tracing::error!("{} {}", self.pubhex, e);
                        }
                    }
                }
            }
        }

        self.clients.write().await.clear();
        tracing::info!("{} shutdown", self.pubhex);

        Ok(())
    }

    async fn accept(&self, addr: TargetAddr<'static>, sck: TcpStream, reconn_cnt: u32) -> Result<()> {
        tracing::info!("{} accept {:?}", self.pubhex, addr);

        let (mut reader, mut writer) = sck.into_split();

        let (myroot, mylast, mycount, greeting) = {
            let blkch = self.blockchain.read().await;
            (
                blkch.root,
                blkch.last,
                blkch.count,
                Message::Greeting {
                    version: self.version.clone(),
                    pubkey: self.pubkey,
                    listen: if let Some(proxy) = &self.cfg.proxy {
                        proxy.announce_by.clone()
                    } else {
                        self.cfg.addr.to_string()
                    },
                    root: blkch.root,
                    last: blkch.last,
                    count: blkch.count,
                    thin: self.cfg.thin,
                    known: self.known.read().await.iter().cloned().collect(),
                },
            )
        };

        ex!(ClientWriter::write_greeting(&mut writer, &greeting).await, source);

        let greeting = ex!(ClientReader::read_greeting(&mut reader).await, source);

        if let Message::Greeting {
            version,
            pubkey,
            listen,
            root,
            last,
            count,
            thin,
            known,
        } = greeting
        {
            if self.version != version {
                return Err(Error::protocol(
                    line!(),
                    module_path!(),
                    "mccloud versions do not match",
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
            let shared = client::shared_secret(&pubkey, &self.prikey);
            let mut reader = ex!(ClientReader::new(reader, &shared), source);
            let cl = ClientInfo {
                // addr,
                thin,
                listen: ex!(listen.into_target_addr(), sync),
                pubkey,
                writer: Mutex::new(ex!(ClientWriter::new(writer, &shared), source)),
            };

            if !thin {
                let new = {
                    let mut k = self.known.write().await;
                    let new = k.insert(pubkey);

                    k.extend(&known);
                    k.remove(&self.pubkey);

                    new
                };

                if new {
                    ex!(self.broadcast(Message::Announce { pubkey }).await, source);
                }
                for new in known {
                    ex!(self.broadcast(Message::Announce { pubkey: new }).await, source);
                }

                if (myroot.is_none() || count > mycount) && last.is_some() {
                    ex!(cl.write(&Message::RequestBlocks { start: mylast }).await, source);
                }
            }

            let pubkey = cl.pubkey;
            let pubhex = self.pubhex.clone();
            let cl = Arc::new(cl);

            let cl0 = cl.clone();
            self.clients.write().await.insert(pubkey, cl0);

            let peer = self.me.upgrade().unwrap();
            let mut rx_shutdown = self.to_shutdown.subscribe();

            tokio::spawn(async move {
                loop {
                    select! {
                        _ = rx_shutdown.recv() => {
                            //peer.clients.write().await.remove(&pubkey);
                            return;
                        }
                        msg = reader.read() => {
                            match msg {
                                Ok(msg) => {
                                    if let Err(e) = peer.on_message(msg, cl.clone()).await {
                                        tracing::error!("{} {}", pubhex, e);
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

                tracing::debug!(
                    "{} disconnect\n{} {}",
                    peer.pubhex,
                    hex::encode(&pubkey),
                    target_addr_to_string(addr.to_owned())
                );
                peer.clients.write().await.remove(&pubkey);
                // peer.known.lock().await.swap_remove(&pubkey);
                peer.trigger_leave(pubkey, addr, reconn_cnt);
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

    fn trigger_leave(&self, pubkey: PubKeyBytes, addr: TargetAddr<'static>, reconn_cnt: u32) {
        let peer = self.me.upgrade().unwrap();
        tokio::spawn(async move {
            if reconn_cnt > 0 {
                tokio::time::sleep(peer.cfg.relationship.reconnect).await;
                tracing::debug!(
                    "{} attempt reconnect\n{} {}",
                    peer.pubhex,
                    hex::encode(&pubkey),
                    target_addr_to_string(addr.to_owned())
                );
                if !peer.to_accept.is_closed() {
                    if let Err(e) = peer.to_accept.send((addr, reconn_cnt - 1, Some(pubkey.clone()))).await {
                        tracing::error!("{} {}", peer.pubhex, e);
                    }
                }
            } else {
                tracing::debug!("{} sent leave: {}", peer.pubhex, hex::encode(&pubkey));
                peer.known.write().await.remove(&pubkey);
                if let Err(e) = peer.broadcast(Message::Leave { pubkey }).await {
                    tracing::error!("{} sent leave: {}", peer.pubhex, e);
                }
            }
        });
    }

    async fn create_next_block(&self, blkch: &mut Blockchain) -> Result<Block> {
        tracing::info!("{} create next block", self.pubhex);

        if let Some(oncb) = &mut *self.on_block_creation.lock().await {
            let cache = blkch.cache.drain().collect();
            blkch.cache = ex!(oncb(cache).await, source);
        }

        // blkch.next_authors
        let Algorithm::Riddle {
            next_candidates,
            forced_restart,
        } = &self.cfg.algorithm;
        let (next_author, all_offline) = {
            let mut nexts = Vec::new();
            let known = self.known.read().await;
            let mut k: Vec<PubKeyBytes> = known.iter().cloned().collect();

            let mut all_offline = true;
            for a in blkch.next_authors.iter() {
                if known.contains(a) {
                    all_offline = false;
                    break;
                }
            }

            while !k.is_empty() && nexts.len() < *next_candidates as _ {
                nexts.push(k.swap_remove(rand::random::<u32>() as usize % k.len()));
            }

            (nexts, all_offline)
        };
        let blk = ex!(blkch.create_block(next_author, self.pubkey, &self.prikey), source);

        let force = *forced_restart && all_offline;
        ex!(blkch.add_block(blk.clone(), force), source);

        Ok(blk)
    }

    async fn check_is_me_next(&self) -> bool {
        let blkch = self.blockchain.read().await;
        let known = self.known.read().await;

        for nxt in blkch.next_authors.iter() {
            if known.contains(nxt) {
                return false;
            } else if *nxt == self.pubkey {
                return true;
            }
        }

        let Algorithm::Riddle { forced_restart, .. } = &self.cfg.algorithm;
        if (blkch.root.is_none() || *forced_restart) && !known.is_empty() && !blkch.cache.is_empty() {
            let mut known: Vec<PubKeyBytes> = known.iter().cloned().collect();
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
                                let mut blkch = peer.blockchain.write().await;
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
            Message::Announce { pubkey } => {
                ex!(self.on_announce(pubkey, cl).await, source);
            }
            Message::Leave { pubkey } => {
                ex!(self.on_leave(pubkey, cl).await, source);
            }
        }

        Ok(())
    }

    async fn perform_share(&self, data: Data, cl: Option<Arc<ClientInfo>>) -> Result<()> {
        let mut blkch = self.blockchain.write().await;

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
        let blk_it = self.blockchain.read().await.get_blocks(start);
        for block in blk_it {
            let block = ex!(block, source);
            ex!(cl.write(&Message::RequestedBlock { block }).await, source);
        }

        Ok(())
    }

    async fn on_requested_block(&self, block: Block) -> Result<()> {
        tracing::info!("{} got block {}", self.pubhex, hex::encode(block.hash));

        let Algorithm::Riddle { forced_restart, .. } = &self.cfg.algorithm;
        ex!(
            self.blockchain.write().await.add_block(block.clone(), *forced_restart),
            source
        );
        if self.last_block_tx.receiver_count() > 0 {
            ex!(self.last_block_tx.send(block), sync);
        }

        Ok(())
    }

    async fn on_share_block(&self, block: Block, cl: Arc<ClientInfo>) -> Result<()> {
        {
            let mut blkch = self.blockchain.write().await;
            let last = blkch.last;
            if last.map(|n| n == block.hash).unwrap_or(false) {
                // we get the same block again, just ignore it
                return Ok(());
            }

            tracing::info!("{} share block {}", self.pubhex, hex::encode(block.hash));
            let mut all_offline = true;
            {
                let known = self.known.read().await;
                for a in blkch.next_authors.iter() {
                    if known.contains(a) {
                        all_offline = false;
                        break;
                    }
                }
            }

            let Algorithm::Riddle { forced_restart, .. } = &self.cfg.algorithm;
            ex!(blkch.add_block(block.clone(), *forced_restart && all_offline), source);
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
        let mut exclude: HashSet<PubKeyBytes> = exclude.into_iter().collect();
        exclude.insert(cl.pubkey);

        let mut possible: Vec<(PubKeyBytes, TargetAddr<'static>)> = {
            let cls = self.clients.read().await;
            cls.iter()
                .filter_map(|(k, v)| {
                    if !v.thin && !exclude.contains(k) {
                        Some((*k, v.listen.to_owned()))
                    } else {
                        None
                    }
                })
                .collect()
        };

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
                let m = possible.swap_remove(rand::random::<u32>() as usize % possible.len());
                to_share.push(m);
            }
        } else {
            to_share = possible;
        }

        let to_share: Vec<(PubKeyBytes, String)> = to_share
            .into_iter()
            .map(|(a, b)| (a, target_addr_to_string(b)))
            .collect();

        if !to_share.is_empty() {
            ex!(
                cl.write(&Message::IntroduceNeighbours { neighbours: to_share }).await,
                source
            );
        }

        Ok(())
    }

    async fn on_introduce_neighbours(&self, neighbours: Vec<(PubKeyBytes, String)>) -> Result<()> {
        // let to_connect: Vec<String> = neighbours.iter().map(|x| hex::encode(x.0)).collect();
        // let to_connect = to_connect.join("\n");
        // tracing::info!(
        //     "{} introduce new neighbours {}\n{}",
        //     self.pubhex,
        //     neighbours.len(),
        //     to_connect
        // );
        let cnt = self.clients.read().await.len();
        if cnt < self.cfg.relationship.count as _ {
            let to_add = self.cfg.relationship.count as usize - cnt;
            for (k, n) in neighbours.into_iter().take(to_add) {
                if !self.clients.read().await.contains_key(&k) {
                    ex!(
                        self.to_accept
                            .send((ex!(n.into_target_addr(), sync), self.cfg.relationship.retry, None))
                            .await,
                        sync
                    );
                }
            }
        }

        Ok(())
    }

    async fn on_announce(&self, pubkey: PubKeyBytes, cl: Arc<ClientInfo>) -> Result<()> {
        if self.pubkey != pubkey {
            let new = self.known.write().await.insert(pubkey);

            if new {
                ex!(self.broadcast_except(Message::Announce { pubkey }, &cl).await, source);
            }
        }

        Ok(())
    }

    async fn on_leave(&self, pubkey: PubKeyBytes, cl: Arc<ClientInfo>) -> Result<()> {
        if self.pubkey != pubkey {
            let known = self.known.write().await.remove(&pubkey);

            if known {
                ex!(self.broadcast_except(Message::Leave { pubkey }, &cl).await, source);
            }
        }

        Ok(())
    }

    /// Returns the public key.
    pub fn pubkey(&self) -> PubKeyBytes {
        self.pubkey.clone()
    }

    /// Returns the hex representation of the public key.
    pub fn pubhex(&self) -> String {
        self.pubhex.clone()
    }

    /// Returns the public keys of the directly connected peers.
    pub async fn client_pubkeys(&self) -> HashSet<PubKeyBytes> {
        let cl = self.clients.read().await;
        cl.keys().cloned().collect()
    }

    /// Returns all known public keys.
    pub async fn known_pubkeys(&self) -> HashSet<PubKeyBytes> {
        self.known.read().await.clone()
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
    pub async fn connect(&self, addr: TargetAddr<'static>) -> Result<()> {
        tracing::info!("{} connect to {:?}", self.pubhex, addr);
        ex!(
            self.to_accept.send((addr, self.cfg.relationship.retry, None)).await,
            sync
        );
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
        self.blockchain.read().await.get_blocks(None)
    }

    pub fn shutdown(&self) -> Result<()> {
        if self.to_shutdown.receiver_count() > 0 {
            ex!(self.to_shutdown.send(true), sync);
        } else {
            tracing::warn!("{} shutdown for no receivers", self.pubhex);
        }
        Ok(())
    }

    pub fn is_shutdown(&self) -> bool {
        self.to_shutdown.receiver_count() == 0
    }
}
