use std::fmt::Display;

// use crate::{HashBytes, PubKeyBytes};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Copy, Clone)]
pub enum ErrorKind {
    Io,
    Addr,
    Sync,
    McRiddle,
    Sqlite,
}

#[derive(Debug)]
pub struct Error {
    source: Option<Box<Error>>,
    kind: ErrorKind,
    line: u32,
    module: String,
    msg: Option<String>,
}

impl Error {
    pub fn source(line: u32, module: &str, e: Error) -> Self {
        Self {
            kind: e.kind,
            source: Some(Box::new(e)),
            line,
            module: module.into(),
            msg: None,
        }
    }

    pub fn parse(line: u32, module: &str, e: std::net::AddrParseError) -> Self {
        Self {
            kind: ErrorKind::Addr,
            source: None,
            line,
            module: module.into(),
            msg: Some(e.to_string()),
        }
    }

    pub fn io(line: u32, module: &str, e: std::io::Error) -> Self {
        Self {
            source: None,
            kind: ErrorKind::Io,
            line,
            module: module.into(),
            msg: Some(e.to_string()),
        }
    }

    pub fn sync<E: Display>(line: u32, module: &str, e: E) -> Self {
        Self {
            source: None,
            kind: ErrorKind::Sync,
            line,
            module: module.into(),
            msg: Some(e.to_string()),
        }
    }

    pub fn riddle(line: u32, module: &str, e: mcriddle::Error) -> Self {
        Self {
            source: None,
            kind: ErrorKind::McRiddle,
            line,
            module: module.into(),
            msg: Some(e.to_string()),
        }
    }

    pub fn sqlite(line: u32, module: &str, e: rusqlite::Error) -> Self {
        Self {
            source: None,
            kind: ErrorKind::Sqlite,
            line,
            module: module.into(),
            msg: Some(e.to_string()),
        }
    }
}

impl std::error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(src) = &self.source {
            write!(f, "{src}\n")?;
        }
        write!(f, "{:?} {} {}: {:?}", self.kind, self.module, self.line, self.msg)
    }
}
