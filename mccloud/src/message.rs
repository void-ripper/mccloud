use borsh::{BorshDeserialize, BorshSerialize};
use k256::{
    schnorr::{
        signature::{Signer, Verifier},
        Signature, SigningKey, VerifyingKey,
    },
    SecretKey,
};

use crate::{
    blockchain::{Block, Data},
    error::{Error, Result},
    ex, HashBytes, PubKeyBytes, SignBytes, Version,
};

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub enum Message {
    Greeting {
        pubkey: PubKeyBytes,
        listen: String,
        root: Option<HashBytes>,
        last: Option<HashBytes>,
        count: u64,
        thin: bool,
        version: Version,
    },
    KeepAlive {
        pubkey: PubKeyBytes,
        count: u64,
        sign: SignBytes,
    },
    ShareData {
        data: Data,
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
    RequestNeighbours {
        count: u32,
        exclude: Vec<PubKeyBytes>,
    },
    IntroduceNeighbours {
        neighbours: Vec<(PubKeyBytes, String)>,
    },
}

impl Message {
    pub fn keepalive(pubkey: &PubKeyBytes, prikey: &SecretKey, count: u64) -> Self {
        let signer = SigningKey::from(prikey);
        let sign: Signature = signer.sign(&count.to_le_bytes());
        let sign_bytes = sign.to_bytes();

        Self::KeepAlive {
            pubkey: *pubkey,
            count,
            sign: sign_bytes,
        }
    }

    pub fn verify(&self) -> Result<bool> {
        match self {
            Self::KeepAlive { pubkey, count, sign } => {
                let verifier = ex!(VerifyingKey::from_bytes(&pubkey[1..]), encrypt);
                let signature = ex!(Signature::try_from(&sign[..]), encrypt);
                ex!(verifier.verify(&count.to_le_bytes(), &signature), encrypt);

                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
