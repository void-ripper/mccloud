use borsh::{BorshDeserialize, BorshSerialize};

use crate::{blockchain::Block, HashBytes, PubKeyBytes};

#[derive(BorshSerialize, BorshDeserialize)]
pub enum Message {
    Greeting {
        pubkey: PubKeyBytes,
        root: Option<HashBytes>,
        last: Option<HashBytes>,
        count: u64,
        known: Vec<PubKeyBytes>,
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
        start: Option<HashBytes>,
    },
    RequestedBlock {
        block: Block,
    },
    ShareBlock {
        block: Block,
    },
}
