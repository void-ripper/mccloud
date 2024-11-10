use borsh::{BorshDeserialize, BorshSerialize};

use crate::{blockchain::Block, PubKeyBytes};

#[derive(BorshSerialize, BorshDeserialize)]
pub enum Message {
    Greeting {
        pubkey: PubKeyBytes,
        root: Option<[u8; 32]>,
    },
    Announce {
        pubkey: PubKeyBytes,
    },
    Remove {
        pubkey: PubKeyBytes,
    },
    ShareData {
        data: Vec<u8>,
    },
    RequestBlocks {
        start: Option<[u8; 32]>,
    },
    RequestedBlock {
        block: Block,
    },
    ShareBlock {
        block: Block,
    },
}
