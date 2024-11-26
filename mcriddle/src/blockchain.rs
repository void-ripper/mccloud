use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    io::{BufReader, Read, Seek, SeekFrom, Write},
    path::PathBuf,
};

use borsh::{BorshDeserialize, BorshSerialize};
use k256::{
    ecdsa::SigningKey,
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
        let file = File::open(&file).unwrap();
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
            while let Some(i) = index_it.next() {
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
pub struct Block {
    pub parent: Option<HashBytes>,
    pub hash: HashBytes,
    pub data: Vec<Vec<u8>>,
    pub next_choice: PubKeyBytes,
    pub author: PubKeyBytes,
    pub sign: SignBytes,
}

pub struct Blockchain {
    index_file: PathBuf,
    db_file: PathBuf,
    pub cache: HashSet<Vec<u8>>,
    pub root: Option<HashBytes>,
    pub last: Option<HashBytes>,
    next_author: Option<PubKeyBytes>,
    pub count: u64,
    block_pos: u64,
}

impl Blockchain {
    pub fn new(folder: &PathBuf) -> Result<Self> {
        if !folder.exists() {
            ex!(std::fs::create_dir_all(&folder), io);
        }

        let index_file = folder.join("index.db");
        let db_file = folder.join("blocks.db");

        let (root, last, block_pos, cnt, next) = if index_file.exists() {
            let mut it = IndexIterator::new(&index_file);

            let mut last = it.next();
            let root = last.as_ref().map(|n| n.hash);
            let mut cnt = if root.is_some() { 1 } else { 0 };

            while let Some(idx) = it.next() {
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

                (Some(last.hash), Some(blk.next_choice), last.pos + last.size)
            } else {
                (None, None, 0)
            };
            (root, last, block_pos, cnt, next)
        } else {
            ex!(OpenOptions::new().create(true).write(true).open(&index_file), io);
            ex!(OpenOptions::new().create(true).write(true).open(&db_file), io);
            (None, None, 0, 0, None)
        };

        Ok(Self {
            index_file,
            db_file,
            cache: HashSet::new(),
            root,
            last,
            next_author: next,
            count: cnt,
            block_pos,
        })
    }

    pub fn get_blocks(&self, start: Option<[u8; 32]>) -> BlockIterator {
        BlockIterator::new(&self.index_file, &self.db_file, start)
    }

    pub fn create_block(&mut self, next: PubKeyBytes, pubkey: PubKeyBytes, secret: &SecretKey) -> Block {
        let data: Vec<Vec<u8>> = self.cache.drain().collect();
        let signer = SigningKey::from(secret);
        let mut hsh = Sha256::new();

        if let Some(parent) = &self.last {
            hsh.update(parent);
        }
        hsh.update(&pubkey);
        hsh.update(&next);
        for d in data.iter() {
            hsh.update(d);
        }

        let hash = hsh.finalize().to_vec();
        let mut hshbytes = [0u8; 32];
        hshbytes.copy_from_slice(&hash);
        let (sign, _) = signer.sign_prehash_recoverable(&hash).unwrap();
        let mut signbytes = [0u8; 64];
        signbytes.copy_from_slice(&sign.to_bytes());

        Block {
            parent: self.last.clone(),
            author: pubkey,
            next_choice: next,
            data,
            hash: hshbytes,
            sign: signbytes,
        }
    }

    pub fn add_block(&mut self, blk: Block) -> Result<()> {
        if self.last != blk.parent {
            return Err(Error::non_child_block(line!(), module_path!(), blk.hash));
        }

        if let Some(next) = &self.next_author {
            if blk.author != *next {
                return Err(Error::unexpected_block_author(
                    line!(),
                    module_path!(),
                    &blk.hash,
                    &blk.author,
                ));
            }
        }

        if self.root.is_none() {
            self.root = Some(blk.hash);
        }

        for d in blk.data.iter() {
            self.cache.remove(d);
        }

        self.last = Some(blk.hash);
        self.count += 1;

        let data = ex!(borsh::to_vec(&blk), io);
        let data = ex!(zstd::stream::encode_all(data.as_slice(), 6), io);
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
