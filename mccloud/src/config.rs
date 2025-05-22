use std::{net::SocketAddr, path::PathBuf, time::Duration};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Relationship {
    /// How many connections a node should have.
    pub count: u32,
    /// In which time intervals to look for new connections.
    pub time: Duration,
    /// The time interval of attempted reconnects.
    pub reconnect: Duration,
    /// How often to retry, after an already established connection is lost.
    pub retry: u32,
}

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Proxy {
    /// The socks5 proxy to use.
    pub proxy: SocketAddr,
    /// The address to use, to connect to this peer. For example an onion address.
    pub announce_by: String,
}

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Algorithm {
    Riddle {
        /// How many candidates are randomly chosen to be able to create the next block.
        next_candidates: u32,
        /// If all next block candidates are offline, a peer can force a new block to restart the
        /// network.
        ///
        /// **Note:** This could lead to a potential security risk, if all next candidates could be
        /// identified and forced offline, so an attacker can force a new block via a malitious modified peer.
        forced_restart: bool,
    },
}

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Config {
    /// The address the peer is listening on.
    pub addr: SocketAddr,
    /// A socks proxy. Mostly used via TOR.
    pub proxy: Option<Proxy>,
    /// The data folder where to save the blockchain.
    pub folder: PathBuf,
    /// The time between keep alive updates.
    pub keep_alive: Duration,
    /// How long to gather new data until a new block is generated.
    pub data_gather_time: Duration,
    /// A thin node does not participate in generating new blocks.
    pub thin: bool,
    /// The relationship config to other nodes.
    pub relationship: Relationship,
    pub algorithm: Algorithm,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: ([0, 0, 0, 0], 29092).into(),
            proxy: None,
            folder: "data".into(),
            keep_alive: Duration::from_millis(1900),
            data_gather_time: Duration::from_millis(750),
            thin: false,
            relationship: Relationship {
                time: Duration::from_secs(10),
                reconnect: Duration::from_secs(15),
                count: 3,
                retry: 3,
            },
            algorithm: Algorithm::Riddle {
                next_candidates: 3,
                forced_restart: true,
            },
        }
    }
}
