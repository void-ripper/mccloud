# mcriddle

This a simple peer to peer blockchain network.

**Features:**
+ Does not waste your electricity, like other algorithms.
+ Secp256k1 for public and private keys for each node.
+ Nodes communicated via AES-256-CBC.
+ Zstd compresses data blocks.
+ Uses Borsh for serialization.

## Example

```rust
let cfg = Config {
  addr: ([127, 0, 0, 1], 29092).into(),
  folder: PathBuf::from("data"),
  keep_alive: Duration::from_millis(250),
  data_gather_time: Duration::from_millis(500),
  thin: false,
  relationship_time: Duration::from_millis(5000),
  relationship_count: 2,
};
let peer = Peer::new(cfg)?;

let mut receiver = peer.last_block_receiver()

tokio::spawn(async move {
  while let Some(block) = receiver.recv().await {
    for data in block.data.iter() {
      data.data; // do something with your data
    }    
  }
});

peer.share(b"some bytes to share").await?;
```

## Development Requirements

Rust toolchain is needed and CMake and LLVM for compiling bundled C dependencies.

### Windows

```powershell
winget install LLVM.LLVM
winget install Kitware.CMake
winget install Rustlang.Rustup
```

### Linux / Mac OS

+ Rustup
+ CMake
+ LLVM

