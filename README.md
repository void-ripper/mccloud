# mcriddle

This a simple peer to peer blockchain network.

**Features:**
+ Secp256k1 for public and private keys for each node.
+ Nodes communicated via AES-256-CBC.
+ Zstd compresses data blocks.
+ Uses Borsh for serialization.

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

