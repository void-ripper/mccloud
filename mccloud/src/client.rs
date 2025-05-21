use aes_gcm_siv::{aead::Aead, Aes256GcmSiv, KeyInit, Nonce};
use k256::{
    ecdh::diffie_hellman,
    elliptic_curve::rand_core::{OsRng, RngCore},
    sha2::{Digest, Sha256},
    PublicKey, SecretKey,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::Mutex,
};
use tokio_socks::TargetAddr;

use crate::{
    error::{Error, Result},
    ex,
    message::Message,
    HashBytes, PubKeyBytes,
};

pub struct ClientWriter {
    pub sck: OwnedWriteHalf,
    pub aes: Aes256GcmSiv,
    pub nonce: u64,
}

pub struct ClientReader {
    pub sck: OwnedReadHalf,
    pub aes: Aes256GcmSiv,
    pub nonce: u64,
}

pub struct ClientInfo {
    // pub addr: SocketAddr,
    pub thin: bool,
    pub listen: TargetAddr<'static>,
    pub pubkey: PubKeyBytes,
    pub writer: Mutex<ClientWriter>,
}

impl ClientInfo {
    pub async fn write(&self, msg: &Message) -> Result<()> {
        let mut w = self.writer.lock().await;
        w.write(msg).await
    }
}

pub fn shared_secret(pubkey: &PubKeyBytes, private_key: &SecretKey) -> HashBytes {
    let pubkey = PublicKey::from_sec1_bytes(pubkey).unwrap();
    let mut shared = Sha256::new();
    shared.update(diffie_hellman(private_key.to_nonzero_scalar(), pubkey.as_affine()).raw_secret_bytes());
    let shared = shared.finalize().to_vec();
    let mut buf: [u8; 32] = [0u8; 32];
    buf.copy_from_slice(&shared);
    buf
}

impl ClientWriter {
    pub fn new(sck: OwnedWriteHalf, shared: &HashBytes) -> Result<Self> {
        let aes = ex!(Aes256GcmSiv::new_from_slice(shared), encrypt);
        Ok(Self { sck, aes, nonce: 1 })
    }

    pub async fn write_greeting(sck: &mut OwnedWriteHalf, msg: &Message) -> Result<()> {
        let data = ex!(borsh::to_vec(msg), io);
        let size = (data.len() as u32).to_le_bytes();
        ex!(sck.write(&size).await, io);
        ex!(sck.write_all(&data).await, io);

        Ok(())
    }

    pub async fn write(&mut self, msg: &Message) -> Result<()> {
        let data = ex!(borsh::to_vec(msg), io);

        let mut iv = [0u8; 12];
        OsRng.fill_bytes(&mut iv);
        let iv = Nonce::from_slice(&iv);

        let nonce = self.nonce.to_le_bytes();
        self.nonce += 1;

        let encrypted = ex!(self.aes.encrypt(&iv, data.as_ref()), encrypt);

        let size = (encrypted.len() as u32).to_le_bytes();
        ex!(self.sck.write(&size).await, io);
        ex!(self.sck.write(&iv).await, io);
        ex!(self.sck.write(&nonce).await, io);
        ex!(self.sck.write_all(&encrypted).await, io);

        Ok(())
    }
}

impl ClientReader {
    pub fn new(sck: OwnedReadHalf, shared: &HashBytes) -> Result<Self> {
        let aes = ex!(Aes256GcmSiv::new_from_slice(shared), encrypt);
        Ok(Self { sck, aes, nonce: 0 })
    }

    pub async fn read_greeting(sck: &mut OwnedReadHalf) -> Result<Message> {
        let mut size_bytes = [0u8; 4];

        ex!(sck.read_exact(&mut size_bytes).await, io);
        let size = u32::from_le_bytes(size_bytes);

        let mut data = vec![0u8; size as usize];
        ex!(sck.read_exact(&mut data).await, io);

        Ok(ex!(borsh::from_slice(&data), io))
    }

    pub async fn read(&mut self) -> Result<Message> {
        let mut size_bytes = [0u8; 4];

        ex!(self.sck.read_exact(&mut size_bytes).await, io);
        let size = u32::from_le_bytes(size_bytes);

        let mut iv = [0u8; 12];
        ex!(self.sck.read_exact(&mut iv).await, io);

        let mut nonce_bytes = [0u8; 8];
        ex!(self.sck.read_exact(&mut nonce_bytes).await, io);
        let nonce = u64::from_le_bytes(nonce_bytes);

        let mut data = vec![0u8; size as usize];
        ex!(self.sck.read_exact(&mut data).await, io);

        if self.nonce >= nonce {
            return Err(Error::protocol(line!(), module_path!(), "nonce to low"));
        }

        self.nonce = nonce;

        let plain = ex!(self.aes.decrypt(Nonce::from_slice(&iv), data.as_ref()), encrypt);

        Ok(ex!(borsh::from_slice(&plain), io))
    }
}
