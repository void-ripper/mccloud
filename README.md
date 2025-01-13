# mccloud

This a simple peer to peer blockchain network.

## Why?

mccloud solves the problem of atomic actions and unique data creation in a peer to peer network.
This project is motivated to have a faster and more energy conservative solution then for,
example Bitcoins proof-of-work algorithm.

## What exactly does or provides mccloud?

+ Atomic actions in peer to peer networks.
+ Data storage via a blockchain.

## The algorithm

For the algorithm to work at least two peers are needed.
If the network starts for the first time, the peer with lowest public key
generates the first block.
After that, every peer who generates a block, picks the next possible peers
who are allowed to create the next block.

## Features

+ Does not waste your electricity, like proof-of-work algorithms.
+ 51% attacks are not possible.
+ Secp256k1 for public and private keys for each node.
+ Nodes communicated via AES-256-CBC.
+ Dynamic block size.
+ Zstd compressed data blocks.
+ Uses [Borsh](https://borsh.io/) for fast and secure serialization.
+ Socks5 support for use via TOR.

## Example

```rust
use std::{
  path::PathBuf,
  time::Duration,
};
use hashbrown::HashMap;
use mccloud::{
  config::Config,
  IntoTargetAddr,
  Peer,
  SignBytes,
  blockchain::Data
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let cfg = Config {
    addr: ([127, 0, 0, 1], 29092).into(),
    folder: PathBuf::from("data"),
    ..Default::default(),
  };
  let peer = Peer::new(cfg)?;

  peer.connect("127.0.0.1:29093".into_target_addr()?.to_owned()).await?;

  peer.set_on_block_creation_cb(|data: HashMap<SignBytes, Data>| Box::pin(async {
    // validate and/or transform the data and/or perform a network atomic actions
    Ok(data)
  })).await;

  let mut receiver = peer.last_block_receiver();

  tokio::spawn(async move {
    while let Ok(block) = receiver.recv().await {
      for data in block.data.iter() {
        // do something with the new data
      } 
    }
  });

  peer.share(b"some bytes to share".to_vec()).await?;

  Ok(())
}
```

## Development Requirements

Rust and Cargo is needed.

### Windows

```powershell
winget install Rustlang.Rustup
```

### Linux / Mac OS

+ Rust + Cargo
