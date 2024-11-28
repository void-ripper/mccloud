use borsh::{BorshDeserialize, BorshSerialize};

use crate::{blockchain::Block, HashBytes, PubKeyBytes, SignBytes};

#[derive(BorshSerialize, BorshDeserialize)]
pub enum Message {
    Greeting {
        pubkey: PubKeyBytes,
        root: Option<HashBytes>,
        last: Option<HashBytes>,
        count: u64,
        thin: bool,
    },
    KeepAlive {
        pubkey: PubKeyBytes,
        sign: SignBytes,
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
