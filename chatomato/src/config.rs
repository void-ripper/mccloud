use std::path::PathBuf;

#[derive(Clone)]
pub struct Config {
    pub addr: String,
    pub data: PathBuf,
}
