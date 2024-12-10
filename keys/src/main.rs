use k256::{
    ecdsa::{signature::SignerMut, Signature, SigningKey},
    elliptic_curve::{rand_core::OsRng, sec1::ToEncodedPoint},
    SecretKey,
};

fn main() {
    let private = SecretKey::random(&mut OsRng);
    let pri_data = private.to_bytes().to_vec();
    let pri_hex = hex::encode(&pri_data);
    let public = private.public_key();
    let pub_data = public.to_encoded_point(true).to_bytes();
    let pub_hex = hex::encode(&pub_data);
    let mut signkey = SigningKey::from(private);
    let sign: Signature = signkey.sign(b"hello");
    let sign = sign.to_bytes();
    let signhex = hex::encode(&sign);

    println!("private: {} {} {}", pri_data.len(), pri_hex.len(), pri_hex);
    println!("public : {} {} {}", pub_data.len(), pub_hex.len(), pub_hex);
    println!("sign   : {} {} {}", sign.len(), signhex.len(), signhex);
}
