use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::PathBuf,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
};

use blockchain::Blockchain;
use client::Client;
use error::{Error, Result};
use k256::{elliptic_curve::sec1::ToEncodedPoint, SecretKey};
use message::Message;
use mio::{
    net::{TcpListener, TcpStream},
    Events, Interest, Poll, Token, Waker,
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

#[derive(Clone)]
pub struct Config {
    pub addr: String,
    pub folder: PathBuf,
}

type Clients = HashMap<Token, Rc<RefCell<Client>>>;

pub struct Listener {
    cfg: Config,
    prikey: SecretKey,
    pubkey: Vec<u8>,
    pubhex: String,
    to_accept: mpsc::Receiver<SocketAddr>,
    to_share: mpsc::Receiver<Vec<u8>>,
    to_shutdown: Arc<AtomicBool>,
    id_pool: usize,
    clients: Clients,
    known: HashSet<Vec<u8>>,
    blockchain: Blockchain,
}

impl Listener {
    pub fn new(
        cfg: Config,
        prikey: SecretKey,
        pubkey: Vec<u8>,
        pubhex: String,
        to_accept: mpsc::Receiver<SocketAddr>,
        to_share: mpsc::Receiver<Vec<u8>>,
        to_shutdown: Arc<AtomicBool>,
    ) -> Self {
        let blockchain = Blockchain::new(&cfg.folder);
        Self {
            cfg,
            prikey,
            pubkey,
            pubhex,
            to_accept,
            to_share,
            to_shutdown,
            id_pool: 10,
            clients: HashMap::new(),
            known: HashSet::new(),
            blockchain,
        }
    }

    fn broadcast_except(&mut self, msg: Message, except: &Rc<RefCell<Client>>) -> Result<()> {
        let except = except.borrow().tk;
        for to in self.clients.values() {
            let mut to = to.borrow_mut();
            if to.tk != except {
                guard!(to.write(&msg), source);
            }
        }

        Ok(())
    }

    fn broadcast(&mut self, msg: Message) -> Result<()> {
        for to in self.clients.values() {
            let mut to = to.borrow_mut();
            guard!(to.write(&msg), source);
        }

        Ok(())
    }

    fn listen(&mut self, waker_tx: mpsc::SyncSender<Waker>) -> Result<()> {
        tracing::info!("{} start", self.pubhex);

        let mut poll = guard!(Poll::new(), io);
        let mut events = Events::with_capacity(128);
        let mut listener = guard!(TcpListener::bind(guard!(self.cfg.addr.parse(), parse)), io);
        let greeting = Message::Greeting {
            pubkey: self.pubkey.clone(),
            root: self.blockchain.root.clone(),
        };

        const SERVER_TK: Token = Token(0);
        guard!(
            poll.registry().register(&mut listener, SERVER_TK, Interest::READABLE),
            io
        );

        const WAKER_TK: Token = Token(1);
        let waker = guard!(Waker::new(poll.registry(), WAKER_TK), io);
        waker_tx.send(waker).unwrap();

        while !self.to_shutdown.load(Ordering::SeqCst) {
            guard!(poll.poll(&mut events, None), io);

            for event in events.iter() {
                match event.token() {
                    SERVER_TK => match listener.accept() {
                        Ok((sck, addr)) => {
                            self.accept(&mut poll, addr, sck)?;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                        Err(e) => return Err(Error::io(line!(), module_path!(), e)),
                    },
                    WAKER_TK => {
                        while let Ok(addr) = self.to_accept.try_recv() {
                            tracing::info!("{} connect to {}", self.pubhex, addr);
                            let sck = guard!(TcpStream::connect(addr), io);
                            guard!(self.accept(&mut poll, addr, sck), source);
                        }

                        while let Ok(data) = self.to_share.try_recv() {
                            if self.blockchain.cache.insert(data.clone()) {
                                let msg = Message::ShareData { data };
                                guard!(self.broadcast(msg), source);
                            }
                        }
                    }
                    tk @ _ => {
                        if let Some(cl) = self.clients.get(&tk).cloned() {
                            if event.is_writable() {
                                guard!(cl.borrow_mut().write(&greeting), source);
                                guard!(
                                    poll.registry().reregister(
                                        &mut cl.borrow_mut().sck,
                                        event.token(),
                                        Interest::READABLE,
                                    ),
                                    io
                                );
                            } else if event.is_readable() {
                                if event.is_read_closed() {
                                    let mut cl = cl.borrow_mut();
                                    let mut pubhex = hex::encode(&cl.pubkey);
                                    pubhex.truncate(12);

                                    tracing::info!("{} remove {} {:?}", self.pubhex, pubhex, cl.addr);

                                    guard!(poll.registry().deregister(&mut cl.sck), io);

                                    self.clients.remove(&tk);
                                    continue;
                                }
                                let msg = cl.borrow_mut().read();
                                let res = self.on_message(msg, &cl);
                                if let Err(e) = res {
                                    tracing::error!("{}: {}", self.pubhex, e);
                                }
                            }
                        }
                    }
                }
            }
        }

        tracing::info!("{} shutdown", self.pubhex);

        Ok(())
    }

    fn accept(&mut self, poll: &mut Poll, addr: SocketAddr, mut sck: TcpStream) -> Result<()> {
        tracing::info!("{} accept {}", self.pubhex, addr);
        let tk = self.id_pool;
        self.id_pool += 1;
        let tk = Token(tk);
        guard!(
            poll.registry()
                .register(&mut sck, tk, Interest::READABLE.add(Interest::WRITABLE)),
            io
        );

        let cl = Rc::new(RefCell::new(Client {
            tk,
            addr,
            sck,
            pubkey: Vec::new(),
            shared: None,
            nonce: 0,
        }));
        self.clients.insert(tk, cl);

        Ok(())
    }

    fn on_message(&mut self, msg: Result<Message>, cl: &Rc<RefCell<Client>>) -> Result<()> {
        match msg {
            Ok(Message::Greeting { pubkey, root }) => {
                {
                    let mut cl = cl.borrow_mut();

                    if root.is_none() && self.blockchain.root.is_none() {
                    } else if self.blockchain.root.is_none() {
                        guard!(cl.write(&Message::RequestBlocks { start: None }), source);
                    }
                    cl.pubkey = pubkey.clone();
                    cl.shared_secret(&self.prikey);
                }
                if pubkey != self.pubkey && self.known.insert(pubkey.clone()) {
                    let msg = Message::Announce { pubkey };
                    guard!(self.broadcast_except(msg, &cl), source);
                }
            }
            Ok(Message::Announce { pubkey }) => {
                if pubkey != self.pubkey && self.known.insert(pubkey.clone()) {
                    let msg = Message::Announce { pubkey };
                    guard!(self.broadcast_except(msg, &cl), source);
                }
            }
            Ok(Message::Remove { pubkey }) => {
                let msg = Message::Remove { pubkey };
                guard!(self.broadcast_except(msg, &cl), source);
            }
            Ok(Message::ShareData { data }) => {
                if self.blockchain.cache.insert(data.clone()) {
                    let msg = Message::ShareData { data };
                    guard!(self.broadcast_except(msg, &cl), source);
                }
            }
            Ok(Message::RequestBlocks { start }) => {
                let blk_it = self.blockchain.get_blocks(start);
                let mut cl = cl.borrow_mut();
                for block in blk_it {
                    guard!(cl.write(&Message::RequestedBlock { block }), source);
                }
            }
            Ok(Message::RequestedBlock { block }) => {
                guard!(self.blockchain.add_block(block), source);
            }
            Ok(Message::ShareBlock { block }) => {
                guard!(self.blockchain.add_block(block.clone()), source);

                guard!(self.broadcast_except(Message::ShareBlock { block }, &cl), source);
            }
            Err(e) => {
                tracing::error!("{} {}", self.pubhex, e);
            }
        }

        Ok(())
    }
}

pub struct Peer {
    pub cfg: Config,
    prikey: SecretKey,
    pubkey: Vec<u8>,
    waker: Arc<Waker>,
    to_shutdown: Arc<AtomicBool>,
    to_accept: mpsc::Sender<SocketAddr>,
    to_share: mpsc::Sender<Vec<u8>>,
}

impl Peer {
    pub fn new(cfg: Config) -> Result<Arc<Self>> {
        let prikey = k256::SecretKey::random(&mut rand::thread_rng());
        let pubkey = prikey.public_key().to_encoded_point(true).as_bytes().to_vec();
        let (waker_tx, waker_rx) = mpsc::sync_channel(1);
        let (to_accept_tx, to_accept_rx) = mpsc::channel();
        let (to_share_tx, to_share_rx) = mpsc::channel();
        let to_shutdown = Arc::new(AtomicBool::new(false));

        let mut pubhex: String = hex::encode(&pubkey);
        pubhex.truncate(12);
        let cfg0 = cfg.clone();
        let prikey0 = prikey.clone();
        let pubkey0 = pubkey.clone();
        let pubhex0 = pubhex.clone();
        let to_shutdown0 = to_shutdown.clone();

        std::thread::spawn(move || {
            let mut listener = Listener::new(cfg0, prikey0, pubkey0, pubhex0, to_accept_rx, to_share_rx, to_shutdown0);
            if let Err(e) = listener.listen(waker_tx) {
                tracing::error!("{} Loop: {e}", pubhex);
            }
        });

        let waker = guard!(waker_rx.recv(), sync);

        Ok(Arc::new(Self {
            prikey,
            pubkey,
            cfg,
            waker: Arc::new(waker),
            to_shutdown,
            to_accept: to_accept_tx,
            to_share: to_share_tx,
        }))
    }

    pub fn connect(&self, addr: SocketAddr) {
        self.to_accept.send(addr).unwrap();
        self.waker.wake().unwrap();
    }

    pub fn share(&self, data: Vec<u8>) {
        self.to_share.send(data).unwrap();
        self.waker.wake().unwrap();
    }

    pub fn shutdown(&self) {
        self.to_shutdown.store(true, Ordering::SeqCst);
        self.waker.wake().unwrap();
    }
}
