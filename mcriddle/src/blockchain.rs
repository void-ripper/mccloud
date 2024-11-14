use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    io::{BufReader, Read, Seek},
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
    HashBytes, PubKeyBytes, SignBytes,
};

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct IndexEntry {
    hash: [u8; 32],
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
        borsh::from_reader::<_, IndexEntry>(&mut self.file).ok()
    }
}

pub struct BlockIterator {
    file: File,
    index_it: IndexIterator,
}

impl BlockIterator {
    pub fn new(index: &PathBuf, db: &PathBuf, start: Option<[u8; 32]>) -> Self {
        let file = File::open(db).unwrap();
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
    type Item = Block;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(idx) = self.index_it.next() {
            let mut buffer = vec![0u8; idx.size as _];
            self.file.seek(std::io::SeekFrom::Start(idx.pos)).unwrap();
            self.file.read_exact(&mut buffer).unwrap();
            return Some(borsh::from_slice(&buffer).unwrap());
        }
        None
    }
}

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct Block {
    pub parent: Option<HashBytes>,
    pub hash: HashBytes,
    pub author: PubKeyBytes,
    pub sign: SignBytes,
    pub data: Vec<Vec<u8>>,
}

pub struct Blockchain {
    index_file: PathBuf,
    db_file: PathBuf,
    pub cache: HashSet<Vec<u8>>,
    pub root: Option<[u8; 32]>,
    pub last: Option<[u8; 32]>,
    pub count: u64,
    block_pos: u64,
}

impl Blockchain {
    pub fn new(folder: &PathBuf) -> Self {
        if !folder.exists() {
            std::fs::create_dir_all(&folder).unwrap();
        }

        let index_file = folder.join("index.db");
        let db_file = folder.join("blocks.db");

        let (root, last, block_pos, cnt) = if index_file.exists() {
            let mut it = IndexIterator::new(&index_file);

            let root = it.next().map(|n| n.hash);
            let mut last = None;
            let mut cnt = 0;

            while let Some(idx) = it.next() {
                last = Some(idx);
                cnt += 1;
            }

            let (last, block_pos) = if let Some(last) = last {
                (Some(last.hash), last.pos + last.size)
            } else {
                (None, 0)
            };
            (root, last, block_pos, cnt)
        } else {
            OpenOptions::new().create(true).write(true).open(&index_file).unwrap();
            OpenOptions::new().create(true).write(true).open(&db_file).unwrap();
            (None, None, 0, 0)
        };

        Self {
            index_file,
            db_file,
            cache: HashSet::new(),
            root,
            last,
            count: cnt,
            block_pos,
        }
    }

    pub fn get_blocks(&self, start: Option<[u8; 32]>) -> BlockIterator {
        BlockIterator::new(&self.index_file, &self.db_file, start)
    }

    pub fn create_block(&mut self, pubkey: PubKeyBytes, secret: &SecretKey) -> Block {
        let data: Vec<Vec<u8>> = self.cache.drain().collect();
        let signer = SigningKey::from(secret);
        let mut hsh = Sha256::new();

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
            data,
            hash: hshbytes,
            sign: signbytes,
        }
    }

    pub fn add_block(&mut self, blk: Block) -> Result<()> {
        if self.last != blk.parent {
            return Err(Error::non_child_block(line!(), module_path!(), blk.hash));
        }

        if self.root.is_none() {
            self.root = Some(blk.hash);
        }

        self.last = Some(blk.hash);
        self.count += 1;

        let data = borsh::to_vec(&blk).map_err(|e| Error::io(line!(), module_path!(), e))?;
        let idx = IndexEntry {
            hash: blk.hash,
            pos: self.block_pos,
            size: data.len() as _,
        };
        self.block_pos += idx.size;

        {
            let idx_file = OpenOptions::new()
                .append(true)
                .open(&self.index_file)
                .map_err(|e| Error::io(line!(), module_path!(), e))?;
            borsh::to_writer(idx_file, &idx).map_err(|e| Error::io(line!(), module_path!(), e))?;
        }

        {
            let db_file = OpenOptions::new()
                .append(true)
                .open(&self.db_file)
                .map_err(|e| Error::io(line!(), module_path!(), e))?;
            borsh::to_writer(db_file, &data).map_err(|e| Error::io(line!(), module_path!(), e))?;
        }

        Ok(())
    }
}
