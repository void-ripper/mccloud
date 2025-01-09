use std::time::SystemTime;

use borsh::{BorshDeserialize, BorshSerialize};
use hashbrown::HashMap;
use k256::{
    ecdsa::signature::hazmat::{PrehashSigner, PrehashVerifier},
    schnorr::{Signature, SigningKey, VerifyingKey},
    sha2::{Digest, Sha256},
    SecretKey,
};
use rand::{rngs::OsRng, RngCore};
// use rayon::{
//     prelude::{ParallelExtend, ParallelIterator},
//     slice::ParallelSliceMut,
// };

use crate::{
    error::{Error, Result},
    ex, PubKeyBytes, SignBytes,
};

const ROCK: u8 = 0;
const PAPER: u8 = 1;
const SCISSOR: u8 = 2;

#[inline]
fn winner(p0: usize, v0: u8, p1: usize, v1: u8) -> usize {
    match (v0, v1) {
        (ROCK, ROCK) => {
            if p0 > p1 {
                p0
            } else {
                p1
            }
        }
        (ROCK, PAPER) => p1,
        (ROCK, SCISSOR) => p0,
        (PAPER, ROCK) => p0,
        (PAPER, PAPER) => {
            if p0 > p1 {
                p0
            } else {
                p1
            }
        }
        (PAPER, SCISSOR) => p1,
        (SCISSOR, ROCK) => p1,
        (SCISSOR, PAPER) => p0,
        (SCISSOR, SCISSOR) => {
            if p0 > p1 {
                p0
            } else {
                p1
            }
        }
        _ => {
            tracing::error!("impossible choices v0({}) v1({})", v0, v1);
            p0
        }
    }
}

struct IntIter {
    pos: usize,
    step: usize,
    end: usize,
}

impl Iterator for IntIter {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos < self.end {
            let val = self.pos;
            self.pos += self.step;
            Some(val)
        } else {
            None
        }
    }
}

///
/// A game of a single node.
///
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct Game {
    /// The public key of the playing node.
    pub author: PubKeyBytes,
    /// The signature over the player rounds.
    pub sign: SignBytes,
    /// The choices of the node for the game rounds.
    pub rounds: Vec<u8>,
}

impl Game {
    pub fn validate(&self) -> Result<()> {
        let verifier = ex!(VerifyingKey::from_bytes(&self.author[1..]), encrypt);
        let signature = ex!(Signature::try_from(&self.sign[..]), encrypt);

        let mut hash = Sha256::new();
        hash.update(&self.rounds);
        let hash = hash.finalize();

        ex!(verifier.verify_prehash(&hash, &signature), encrypt);

        Ok(())
    }
}

fn game_rounds(roster_cnt: usize) -> usize {
    (roster_cnt as f64).log2().ceil() as _
}

fn calc_tree_count(roster_cnt: usize) -> usize {
    let count = game_rounds(roster_cnt);
    let count = 1 << count;

    count * 2 - 1
}

///
/// An abstraction of the Highlander algorithm.
///
pub struct Highlander {
    pub roster: HashMap<PubKeyBytes, Option<Vec<u8>>>,
    rounds: usize,
}

impl Highlander {
    pub fn new() -> Self {
        Self {
            roster: HashMap::new(),
            rounds: 0,
        }
    }

    pub fn add_to_roster(&mut self, pubkey: PubKeyBytes) {
        self.roster.insert(pubkey, None);
        self.rounds = game_rounds(self.roster.len());
    }

    pub fn remove_to_roster(&mut self, pubkey: &PubKeyBytes) {
        self.roster.remove_entry(pubkey);
        self.rounds = game_rounds(self.roster.len());
    }

    pub fn populate_roster<'a, T: Iterator<Item = &'a PubKeyBytes>>(&mut self, iter: T) {
        self.roster.extend(iter.map(|id| (id.clone(), None)));
        self.rounds = game_rounds(self.roster.len());
    }

    // pub fn par_populate_roster<'a, T: ParallelIterator<Item = &'a PubKeyBytes>>(&mut self, iter: T) {
    //     self.roster.par_extend(iter.map(|id| (id.clone(), None)));
    //     self.rounds = game_rounds(self.roster.len());
    // }

    pub fn add_game(&mut self, game: Game) -> bool {
        // tracing::debug!("add game from: {}", hex::encode(&game.author));

        // let count = game_rounds(self.roster.len());

        if game.rounds.len() == self.rounds {
            if let Some(maybe) = self.roster.get(&game.author) {
                if maybe.is_none() {
                    if let Err(e) = game.validate() {
                        tracing::error!(
                            "game could not be validated\nauthor: {}\n{}",
                            hex::encode(&game.author),
                            e
                        );
                        false
                    } else {
                        self.roster.insert(game.author, Some(game.rounds));
                        true
                    }
                } else {
                    tracing::warn!("got already a game for\nauthor: {}", hex::encode(&game.author));
                    false
                }
            } else {
                tracing::error!(
                    "game author is not part of the game\nauthor: {}",
                    hex::encode(&game.author)
                );
                false
            }
        } else {
            tracing::error!(
                "round length do not match:\nexpected: {}\ngame: {}\nauthor: {}",
                self.rounds,
                game.rounds.len(),
                hex::encode(&game.author)
            );
            false
        }
    }

    pub fn create_game(&self, pubkey: &PubKeyBytes, key: &SecretKey) -> Result<Game> {
        // tracing::debug!("create game for: {}", hex::encode(&key.public_key));

        let count = game_rounds(self.roster.len());
        let mut buf = vec![0u8; count];

        OsRng.fill_bytes(&mut buf);

        for i in 0..count {
            buf[i] %= 3;
        }

        let mut hash = Sha256::new();
        hash.update(&buf);
        let hash = hash.finalize();
        let signer = SigningKey::from(key);
        let sign = ex!(signer.sign_prehash(&hash), encrypt);

        Ok(Game {
            author: pubkey.clone(),
            sign: sign.to_bytes(),
            rounds: buf,
        })
    }

    pub fn is_filled(&self) -> bool {
        if self.roster.len() == 0 {
            // tracing::error!("is filled should never be checked if roster is not populated!");
            return false;
        }

        for val in self.roster.values() {
            if val.is_none() {
                return false;
            }
        }

        true
    }

    pub fn evaluate(&mut self) -> PubKeyBytes {
        let count = calc_tree_count(self.roster.len());
        let mut tree: Vec<Option<usize>> = vec![None; count];

        let start = SystemTime::now();
        let mut roster: Vec<(PubKeyBytes, Vec<u8>)> = self.roster.drain().map(|k| (k.0, k.1.unwrap())).collect();
        let dur = start.elapsed().unwrap();
        tracing::debug!("roster collect -> {:?}", dur);

        let start = SystemTime::now();
        // if roster.len() < 4096 {
        roster.sort_by_cached_key(|x| x.0.clone());
        // } else {
        //     roster.par_sort_by_cached_key(|x| x.0.clone());
        // }
        let dur = start.elapsed().unwrap();
        tracing::debug!("roster stort -> {:?}", dur);

        for i in 0..roster.len() {
            tree[i] = Some(i);
        }

        let mut lvl = 0;
        let mut offset = 0;
        let mut count = (count + 1) / 2;

        let start = SystemTime::now();
        while count > 1 {
            let rng = IntIter {
                pos: 0,
                end: count,
                step: 2,
            };
            for i in rng {
                if let Some(p0) = tree[offset + i] {
                    let v0 = roster[p0].1[lvl];

                    let w = if let Some(p1) = tree[offset + i + 1] {
                        let v1 = roster[p1].1[lvl];

                        winner(p0, v0, p1, v1)
                    } else {
                        p0
                    };

                    tree[i / 2 + count + offset] = Some(w);
                }
            }

            offset += count;
            count /= 2;
            lvl += 1;
        }
        let dur = start.elapsed().unwrap();
        tracing::debug!("tree evaluation -> {:?}", dur);

        // tracing::debug!("tree {:?}", tree);
        let winner = tree.last().unwrap().clone().unwrap();
        let winner_pubkey = roster[winner].0;
        tracing::debug!("winner {}", hex::encode(&winner_pubkey));

        winner_pubkey
    }
}
