use std::net::SocketAddr;

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
    ex, HashBytes, PubKeyBytes, SignBytes,
};

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub enum Message {
    Greeting {
        pubkey: PubKeyBytes,
        listen: SocketAddr,
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
        neighbours: Vec<(PubKeyBytes, SocketAddr)>,
    },
}

impl Message {
    pub fn keepalive(pubkey: &PubKeyBytes, prikey: &SecretKey) -> Self {
        let signer = SigningKey::from(prikey);
        let sign: Signature = signer.sign(pubkey);
        let sign_bytes = sign.to_bytes();

        Self::KeepAlive {
            pubkey: *pubkey,
            sign: sign_bytes,
        }
    }

    pub fn verify(&self) -> Result<bool> {
        match self {
            Self::KeepAlive { pubkey, sign } => {
                let verifier = ex!(VerifyingKey::from_bytes(&pubkey[1..]), encrypt);
                let signature = ex!(Signature::try_from(&sign[..]), encrypt);
                ex!(verifier.verify(pubkey, &signature), encrypt);

                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
