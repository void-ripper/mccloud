use std::{net::SocketAddr, path::PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};

#[derive(Deserialize, Clone)]
pub struct Config {
    pub addr: SocketAddr,
    pub data: PathBuf,
    pub clients: Vec<String>,
}

pub fn load() -> Result<Config> {
    let cfgfile = PathBuf::from("data/config.toml");

    if cfgfile.exists() {
        let data = std::fs::read_to_string(&cfgfile).map_err(|e| Error::io(line!(), module_path!(), e))?;
        Ok(toml::from_str(&data).map_err(|e| Error::sync(line!(), module_path!(), e))?)
    } else {
        std::fs::write(
            &cfgfile,
            r#"addr = "127.0.0.1:29092"
data = "data/"
clients = []"#,
        )
        .map_err(|e| Error::io(line!(), module_path!(), e))?;
        Ok(Config {
            addr: "0.0.0.0:29092".parse().unwrap(),
            data: "data".into(),
            clients: Vec::new(),
        })
    }
}
