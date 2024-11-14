use std::net::SocketAddr;

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
    guard,
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
    pub(crate) nonce: u64,
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
        let data = guard!(borsh::to_vec(msg), io);

        if let Some(shared) = &self.shared {
            let mut iv = [0u8; 16];
            OsRng.fill_bytes(&mut iv);
            let enc = guard!(AesCbcEnc::new_from_slices(shared, &iv), encrypt);
            let encrypted = enc.encrypt_padded_vec_mut::<Pkcs7>(&data);

            let size = (encrypted.len() as u32).to_le_bytes();
            guard!(self.sck.write(&size).await, io);
            guard!(self.sck.write(&iv).await, io);
            guard!(self.sck.write_all(&data).await, io);
        } else {
            let size = (data.len() as u32).to_ne_bytes();
            guard!(self.sck.write(&size).await, io);
            guard!(self.sck.write_all(&data).await, io);
        }

        Ok(())
    }

    pub async fn read(sck: &mut OwnedReadHalf, shared: &Option<HashBytes>) -> Result<Message> {
        let mut size_bytes = [0u8; 4];
        guard!(sck.read_exact(&mut size_bytes).await, io);
        let size = u32::from_le_bytes(size_bytes);

        let data = if let Some(shared) = &shared {
            let mut iv = [0u8; 16];
            guard!(sck.read_exact(&mut iv).await, io);

            let mut data = vec![0u8; size as usize];
            guard!(sck.read_exact(&mut data).await, io);

            let dec = guard!(AesCbcDec::new_from_slices(shared, &iv), encrypt);
            let data: Vec<u8> = guard!(dec.decrypt_padded_vec_mut::<Pkcs7>(&data), padding);

            data
        } else {
            let mut data = vec![0u8; size as usize];
            guard!(sck.read_exact(&mut data).await, io);

            data
        };

        Ok(guard!(borsh::from_slice(&data), io))
    }
}
