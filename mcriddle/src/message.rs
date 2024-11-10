use borsh::{BorshDeserialize, BorshSerialize};

use crate::blockchain::Block;

#[derive(BorshSerialize, BorshDeserialize)]
pub enum Message {
    Greeting { pubkey: Vec<u8>, root: Option<[u8; 32]> },
    Announce { pubkey: Vec<u8> },
    Remove { pubkey: Vec<u8> },
    ShareData { data: Vec<u8> },
    RequestBlocks { start: Option<[u8; 32]> },
    RequestedBlock { block: Block },
    ShareBlock { block: Block },
}
