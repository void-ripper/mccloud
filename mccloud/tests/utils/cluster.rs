#![allow(dead_code)]

use std::{path::PathBuf, sync::Arc};

use mccloud::{config::Config, Peer};

use crate::utils::configs::{ClientConfigs, ServerConfigs};

pub struct Cluster {
    server_configs: ServerConfigs,
    client_configs: ClientConfigs,
    fat_peers: Vec<Arc<Peer>>,
    thin_peers: Vec<Arc<Peer>>,
}

impl Cluster {
    pub fn new(seed: u16) -> Self {
        let cl = Self {
            server_configs: ServerConfigs::new(seed),
            client_configs: ClientConfigs::new(seed),
            fat_peers: Vec::new(),
            thin_peers: Vec::new(),
        };

        cl
    }

    pub fn create(&mut self, cnt: usize, thin: bool) -> Vec<Arc<Peer>> {
        let iter: &mut dyn Iterator<Item = Config> = if thin {
            &mut self.client_configs
        } else {
            &mut self.server_configs
        };

        let peers: Vec<Arc<Peer>> = iter.take(cnt).map(|c| Peer::new(c).unwrap()).collect();

        if thin {
            self.thin_peers.extend(peers.iter().cloned());
        } else {
            self.fat_peers.extend(peers.iter().cloned());
        }

        peers
    }

    pub fn shutdown(&self) {
        for p in &self.thin_peers {
            p.shutdown().unwrap();
        }

        for p in &self.fat_peers {
            p.shutdown().unwrap();
        }
    }

    pub fn cleanup(&self) {
        fn remove(p: &Arc<Peer>) {
            let mut pbuf = PathBuf::new();
            pbuf.push(&p.cfg.folder);
            if pbuf.exists() && pbuf.is_dir() {
                std::fs::remove_dir_all(pbuf).unwrap();
            }
        }

        self.thin_peers.iter().for_each(remove);
        self.fat_peers.iter().for_each(remove);
    }
}
