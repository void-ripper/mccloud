use std::path::PathBuf;

pub struct Database {
    db: rusqlite::Connection,
}

impl Database {
    pub fn new() -> Self {
        let existed = PathBuf::from("data.db").exists();
        let db = rusqlite::Connection::open("data.db").unwrap();

        if !existed {}

        Self { db }
    }
}
