use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    io::{BufReader, Read, Seek},
    path::PathBuf,
};

use borsh::{BorshDeserialize, BorshSerialize};

use crate::error::{Error, Result};

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
    start: Option<[u8; 32]>,
    index_it: IndexIterator,
}

impl BlockIterator {
    pub fn new(index: &PathBuf, db: &PathBuf, start: Option<[u8; 32]>) -> Self {
        let file = File::open(db).unwrap();
        Self {
            file,
            start,
            index_it: IndexIterator::new(index),
        }
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
    pub parent: Option<[u8; 32]>,
    pub hash: [u8; 32],
    pub author: Vec<u8>,
    pub sign: Vec<u8>,
    pub data: Vec<u8>,
}

pub struct Blockchain {
    index_file: PathBuf,
    db_file: PathBuf,
    pub cache: HashSet<Vec<u8>>,
    pub root: Option<[u8; 32]>,
    pub last: Option<[u8; 32]>,
    block_pos: u64,
}

impl Blockchain {
    pub fn new(folder: &PathBuf) -> Self {
        if !folder.exists() {
            std::fs::create_dir_all(&folder).unwrap();
        }

        let index_file = folder.join("index.db");

        let (root, last, block_pos) = if index_file.exists() {
            let mut it = IndexIterator::new(&index_file);

            let root = it.next().map(|n| n.hash);
            let last = it.last();
            let (last, block_pos) = if let Some(last) = last {
                (Some(last.hash), last.pos + last.size)
            } else {
                (None, 0)
            };
            (root, last, block_pos)
        } else {
            (None, None, 0)
        };

        Self {
            index_file,
            db_file: folder.join("blocks.db"),
            cache: HashSet::new(),
            root,
            last,
            block_pos,
        }
    }

    pub fn get_blocks(&self, start: Option<[u8; 32]>) -> BlockIterator {
        BlockIterator::new(&self.index_file, &self.db_file, start)
    }

    pub fn add_block(&mut self, blk: Block) -> Result<()> {
        if self.last != blk.parent {
            return Err(Error::non_child_block(line!(), module_path!(), blk.hash));
        }

        if self.root.is_none() {
            self.root = Some(blk.hash);
        }

        self.last = Some(blk.hash);

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
