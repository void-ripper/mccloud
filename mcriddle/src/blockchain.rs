use std::{
    fs::{File, OpenOptions},
    io::{BufReader, Read, Seek, SeekFrom, Write},
    path::PathBuf,
};

use borsh::{BorshDeserialize, BorshSerialize};
use hashbrown::HashMap;
use k256::{
    ecdsa::signature::hazmat::{PrehashSigner, PrehashVerifier},
    schnorr::{Signature, SigningKey, VerifyingKey},
    sha2::{Digest, Sha256},
    SecretKey,
};

use crate::{
    error::{Error, Result},
    ex, HashBytes, PubKeyBytes, SignBytes,
};

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct IndexEntry {
    hash: HashBytes,
    pos: u64,
    size: u64,
}

pub struct IndexIterator {
    file: BufReader<File>,
}

impl IndexIterator {
    pub fn new(file: &PathBuf) -> Self {
        let file = File::open(file).unwrap();
        let file = BufReader::new(file);
        Self { file }
    }
}

impl Iterator for IndexIterator {
    type Item = IndexEntry;

    fn next(&mut self) -> Option<Self::Item> {
        // borsh::from_reader::<_, IndexEntry>(&mut self.file).ok()
        IndexEntry::deserialize_reader(&mut self.file).ok()
    }
}

pub struct BlockIterator {
    file: BufReader<File>,
    index_it: IndexIterator,
}

impl BlockIterator {
    pub fn new(index: &PathBuf, db: &PathBuf, start: Option<HashBytes>) -> Self {
        let file = File::open(db).unwrap();
        let file = BufReader::new(file);
        let mut index_it = IndexIterator::new(index);

        if let Some(start) = start {
            for i in index_it.by_ref() {
                if i.hash == start {
                    break;
                }
            }
        }

        Self { file, index_it }
    }
}

impl Iterator for BlockIterator {
    type Item = Result<Block>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(idx) = self.index_it.next() {
            let mut buffer = vec![0u8; idx.size as _];
            if let Err(e) = self.file.seek(std::io::SeekFrom::Start(idx.pos)) {
                return Some(Err(Error::io(line!(), module_path!(), e)));
            }
            if let Err(e) = self.file.read_exact(&mut buffer) {
                return Some(Err(Error::io(line!(), module_path!(), e)));
            }
            match zstd::stream::decode_all(buffer.as_slice()) {
                Ok(buffer) => match borsh::from_slice(&buffer) {
                    Ok(block) => {
                        return Some(Ok(block));
                    }
                    Err(e) => {
                        return Some(Err(Error::io(line!(), module_path!(), e)));
                    }
                },
                Err(e) => {
                    return Some(Err(Error::io(line!(), module_path!(), e)));
                }
            }
        }

        None
    }
}

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct Data {
    pub data: Vec<u8>,
    /// The node which created the data.
    pub author: PubKeyBytes,
    /// The signed
    pub sign: SignBytes,
}

impl Data {
    pub fn new(data: Vec<u8>, author: &PubKeyBytes, secret: &SecretKey) -> Result<Self> {
        let signer = SigningKey::from(secret);
        let mut sha = Sha256::new();

        sha.update(author);
        sha.update(&data);

        let mut hshbytes = [0u8; 32];
        let hash = sha.finalize();
        hshbytes.copy_from_slice(&hash);

        let sign = ex!(signer.sign_prehash(&hshbytes), encrypt);
        let signbytes = sign.to_bytes();

        Ok(Self {
            data,
            author: *author,
            sign: signbytes,
        })
    }

    pub fn verify(&self) -> Result<()> {
        let mut sha = Sha256::new();

        sha.update(self.author);
        sha.update(&self.data);
        let hash = sha.finalize();

        let verifier = ex!(VerifyingKey::from_bytes(&self.author[1..]), encrypt);
        let sign = ex!(Signature::try_from(&self.sign[..]), encrypt);
        ex!(verifier.verify_prehash(&hash, &sign), encrypt);

        Ok(())
    }
}

/// A single block in the block chain.
#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct Block {
    /// The block that comes before this one.
    pub parent: Option<HashBytes>,
    /// The hash of this block.
    pub hash: HashBytes,
    /// The data of this block.
    pub data: Vec<Data>,
    /// The next block author.
    pub next_choices: Vec<PubKeyBytes>,
    /// The current block author.
    pub author: PubKeyBytes,
    /// The signature of the block data, created with the private key of the author.
    pub sign: SignBytes,
}

impl Block {
    pub fn verify(&self) -> Result<bool> {
        let hash = hash_data(&self.parent, &self.author, &self.next_choices, &self.data);
        let verifier = ex!(VerifyingKey::from_bytes(&self.author[1..]), encrypt);
        let sign = ex!(Signature::try_from(&self.sign[..]), encrypt);
        ex!(verifier.verify_prehash(&hash, &sign), encrypt);

        Ok(true)
    }
}

pub struct Blockchain {
    index_file: PathBuf,
    db_file: PathBuf,
    pub cache: HashMap<SignBytes, Data>,
    pub root: Option<HashBytes>,
    pub last: Option<HashBytes>,
    pub next_authors: Vec<PubKeyBytes>,
    pub count: u64,
    block_pos: u64,
}

fn hash_data(last: &Option<HashBytes>, pubkey: &PubKeyBytes, next: &[PubKeyBytes], data: &[Data]) -> Vec<u8> {
    let mut hsh = Sha256::new();

    if let Some(parent) = last {
        hsh.update(parent);
    }
    hsh.update(pubkey);

    for n in next {
        hsh.update(n);
    }

    for d in data.iter() {
        hsh.update(&d.data);
        hsh.update(d.author);
        hsh.update(d.sign);
    }

    hsh.finalize().to_vec()
}

impl Blockchain {
    pub fn new(folder: &PathBuf) -> Result<Self> {
        if !folder.exists() {
            ex!(std::fs::create_dir_all(folder), io);
        }

        let index_file = folder.join("index.db");
        let db_file = folder.join("blocks.db");

        let (root, last, block_pos, cnt, next) = if index_file.exists() {
            let mut it = IndexIterator::new(&index_file);

            let mut last = it.next();
            let root = last.as_ref().map(|n| n.hash);
            let mut cnt = if root.is_some() { 1 } else { 0 };

            for idx in it {
                last = Some(idx);
                cnt += 1;
            }

            let (last, next, block_pos) = if let Some(last) = last {
                let mut file = ex!(File::open(&db_file), io);
                ex!(file.seek(SeekFrom::Start(last.pos)), io);
                let mut buffer = vec![0u8; last.size as _];
                ex!(file.read_exact(&mut buffer), io);
                let buffer = ex!(zstd::stream::decode_all(buffer.as_slice()), io);
                let blk: Block = borsh::from_slice(&buffer).unwrap();

                (Some(last.hash), blk.next_choices, last.pos + last.size)
            } else {
                (None, Vec::new(), 0)
            };
            (root, last, block_pos, cnt, next)
        } else {
            ex!(
                OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .write(true)
                    .open(&index_file),
                io
            );
            ex!(
                OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .write(true)
                    .open(&db_file),
                io
            );
            (None, None, 0, 0, Vec::new())
        };

        Ok(Self {
            index_file,
            db_file,
            cache: HashMap::new(),
            root,
            last,
            next_authors: next,
            count: cnt,
            block_pos,
        })
    }

    pub fn get_blocks(&self, start: Option<[u8; 32]>) -> BlockIterator {
        BlockIterator::new(&self.index_file, &self.db_file, start)
    }

    /// Creates a new block. The block is *not* added to the block chain.
    pub fn create_block(&mut self, next: Vec<PubKeyBytes>, pubkey: PubKeyBytes, secret: &SecretKey) -> Result<Block> {
        let data: Vec<Data> = self.cache.drain().map(|(_k, v)| v).collect();
        let signer = SigningKey::from(secret);
        let hash = hash_data(&self.last, &pubkey, &next, &data);

        let mut hshbytes = [0u8; 32];
        hshbytes.copy_from_slice(&hash);
        let sign = ex!(signer.sign_prehash(&hash), encrypt);
        let signbytes = sign.to_bytes();

        Ok(Block {
            parent: self.last,
            author: pubkey,
            next_choices: next,
            data,
            hash: hshbytes,
            sign: signbytes,
        })
    }

    /// Adds a new block to the block chain.
    pub fn add_block(&mut self, blk: Block) -> Result<()> {
        if self.last != blk.parent {
            return Err(Error::non_child_block(line!(), module_path!(), blk.hash));
        }

        if self.root.is_some() && !self.next_authors.contains(&blk.author) {
            return Err(Error::unexpected_block_author(
                line!(),
                module_path!(),
                &blk.hash,
                &blk.author,
                &self.next_authors,
            ));
        }

        ex!(blk.verify(), source);

        if self.root.is_none() {
            self.root = Some(blk.hash);
        }

        for d in blk.data.iter() {
            self.cache.remove(&d.sign);
        }

        self.last = Some(blk.hash);
        self.next_authors = blk.next_choices.clone();
        self.count += 1;

        let data = ex!(borsh::to_vec(&blk), io);
        let data = ex!(zstd::stream::encode_all(data.as_slice(), 19), io);
        let idx = IndexEntry {
            hash: blk.hash,
            pos: self.block_pos,
            size: data.len() as _,
        };
        self.block_pos += idx.size;

        {
            let idx_file = ex!(OpenOptions::new().append(true).open(&self.index_file), io);
            ex!(borsh::to_writer(idx_file, &idx), io);
        }

        {
            let mut db_file = ex!(OpenOptions::new().append(true).open(&self.db_file), io);
            ex!(db_file.write_all(&data), io);
        }

        Ok(())
    }
}
