use std::{io::Read, net::SocketAddr};

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use k256::{
    ecdh::diffie_hellman,
    elliptic_curve::rand_core::{OsRng, RngCore},
    sha2::{Digest, Sha256},
    PublicKey, SecretKey,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
};

use crate::{
    error::{Error, Result},
    ex,
    message::Message,
    HashBytes, PubKeyBytes,
};

pub type AesCbcEnc = cbc::Encryptor<aes::Aes256>;
pub type AesCbcDec = cbc::Decryptor<aes::Aes256>;

pub struct Client {
    pub(crate) addr: SocketAddr,
    pub(crate) pubkey: PubKeyBytes,
    pub(crate) sck: OwnedWriteHalf,
    pub(crate) shared: Option<HashBytes>,
    pub(crate) tx_nonce: u64,
}

impl Client {
    pub fn shared_secret(&mut self, private_key: &SecretKey) {
        let pubkey = PublicKey::from_sec1_bytes(&self.pubkey).unwrap();
        let mut shared = Sha256::new();
        shared.update(diffie_hellman(private_key.to_nonzero_scalar(), pubkey.as_affine()).raw_secret_bytes());
        let shared = shared.finalize().to_vec();
        let mut buf: [u8; 32] = [0u8; 32];
        buf.copy_from_slice(&shared);
        self.shared = Some(buf);
    }

    pub async fn write(&mut self, msg: &Message) -> Result<()> {
        let data = ex!(borsh::to_vec(msg), io);
        self.tx_nonce += 1;
        let nonce = self.tx_nonce;
        let nonce_bytes = nonce.to_le_bytes();

        if let Some(shared) = &self.shared {
            let mut iv = [0u8; 16];
            OsRng.fill_bytes(&mut iv);
            let enc = ex!(AesCbcEnc::new_from_slices(shared, &iv), encrypt);
            let data = [&nonce_bytes, data.as_slice()].concat();
            let encrypted = enc.encrypt_padded_vec_mut::<Pkcs7>(&data);

            let size = (encrypted.len() as u32).to_le_bytes();
            ex!(self.sck.write(&size).await, io);
            ex!(self.sck.write(&iv).await, io);
            ex!(self.sck.write_all(&encrypted).await, io);
        } else {
            let size = (data.len() as u32).to_le_bytes();
            ex!(self.sck.write(&size).await, io);
            ex!(self.sck.write(&nonce_bytes).await, io);
            ex!(self.sck.write_all(&data).await, io);
        }

        Ok(())
    }

    pub async fn read(sck: &mut OwnedReadHalf, shared: &Option<HashBytes>) -> Result<(u64, Message)> {
        let mut size_bytes = [0u8; 4];

        ex!(sck.read_exact(&mut size_bytes).await, io);
        let size = u32::from_le_bytes(size_bytes);

        let (nonce, data) = if let Some(shared) = &shared {
            let mut iv = [0u8; 16];
            ex!(sck.read_exact(&mut iv).await, io);

            let mut data = vec![0u8; size as usize];
            ex!(sck.read_exact(&mut data).await, io);

            let dec = ex!(AesCbcDec::new_from_slices(shared, &iv), encrypt);
            let data: Vec<u8> = ex!(dec.decrypt_padded_vec_mut::<Pkcs7>(&data), padding);

            let (nonce, data) = data.split_at(8);
            let mut nonce_bytes = [0u8; 8];
            nonce_bytes.copy_from_slice(nonce);
            let nonce = u64::from_le_bytes(nonce_bytes);

            (nonce, data.to_vec())
        } else {
            let mut nonce_bytes = [0u8; 8];
            ex!(sck.read_exact(&mut nonce_bytes).await, io);
            let nonce = u64::from_le_bytes(nonce_bytes);

            let mut data = vec![0u8; size as usize];
            ex!(sck.read_exact(&mut data).await, io);

            (nonce, data)
        };

        Ok((nonce, ex!(borsh::from_slice(&data), io)))
    }
}
