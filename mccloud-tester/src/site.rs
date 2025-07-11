use askama::Template;

use crate::{BlockData, PeerData};

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexSite {
    pub peers: Vec<PeerData>,
    pub flakies: Vec<PeerData>,
    pub spawn_count: u32,
    pub is_flaking: bool,
    pub flake_time: u32,
    pub target: Option<PeerData>,
    pub blocks: Vec<BlockData>,
}

impl IndexSite {
    fn is_selected(&self, p: &PeerData) -> bool {
        if let Some(t) = &self.target {
            return t.id == p.id;
        }

        false
    }
}
