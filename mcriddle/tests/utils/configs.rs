#![allow(dead_code)]

use mcriddle::Config;


pub struct ServerConfigs {
    port: u16,
}

impl ServerConfigs {
    pub fn new(seed: u16) -> Self {
        Self {
            port: seed,
        }
    }
}

impl Iterator for ServerConfigs {
    type Item = Config;

    fn next(&mut self) -> Option<Self::Item> {
        let port = self.port + 29092;
        self.port += 1;

        Some(Config {
            addr: ([127, 0, 0, 1], port).into(),
            folder: format!("data/test{:02}", port).into(),
            ..Default::default()
        })
    }
}

pub struct ClientConfigs {
    port: u16,
}

impl ClientConfigs {
    pub fn new(seed: u16) -> Self {
        Self {
            port: seed,
        }
    }
}

impl Iterator for ClientConfigs {
    type Item = Config;

    fn next(&mut self) -> Option<Self::Item> {
        let port = self.port + 49093;
        self.port += 1;

        Some(Config {
            addr: ([127, 0, 0, 1], port).into(),
            folder: format!("data/client{:02}", port).into(),
            ..Default::default()
        })
    }
}
