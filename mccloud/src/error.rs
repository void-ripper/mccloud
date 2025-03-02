use std::{fmt::Display, sync::Arc};

use crate::{HashBytes, PubKeyBytes};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    Io,
    Disconnect,
    Addr,
    Sync,
    Encryption,
    Blockchain,
    Protocol,
    Extern,
}

#[derive(Debug)]
pub struct Error {
    pub source: Option<Arc<Error>>,
    pub kind: ErrorKind,
    pub line: u32,
    pub module: String,
    pub msg: Option<String>,
}

impl Error {
    pub fn source(line: u32, module: &str, e: Error) -> Self {
        Self {
            kind: e.kind,
            source: Some(Arc::new(e)),
            line,
            module: module.into(),
            msg: None,
        }
    }

    pub fn protocol(line: u32, module: &str, txt: &str) -> Self {
        Self {
            kind: ErrorKind::Protocol,
            source: None,
            line,
            module: module.into(),
            msg: Some(txt.into()),
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
        let kind = match e.kind() {
            std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::BrokenPipe => ErrorKind::Disconnect,
            _ => ErrorKind::Io,
        };
        Self {
            source: None,
            kind,
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

    pub fn encrypt<E: Display>(line: u32, module: &str, e: E) -> Self {
        Self {
            source: None,
            kind: ErrorKind::Encryption,
            line,
            module: module.into(),
            msg: Some(e.to_string()),
        }
    }

    pub fn non_child_block(line: u32, module: &str, hsh: HashBytes) -> Self {
        Self {
            source: None,
            kind: ErrorKind::Blockchain,
            line,
            module: module.into(),
            msg: Some(format!("block ({}) is not child of current block", hex::encode(hsh))),
        }
    }

    pub fn unexpected_block_author(
        line: u32,
        module: &str,
        hsh: &HashBytes,
        author: &PubKeyBytes,
        expected: &[PubKeyBytes],
    ) -> Self {
        let expect: Vec<String> = expected.iter().map(hex::encode).collect();
        let expect = expect.join("\n");
        Self {
            source: None,
            kind: ErrorKind::Blockchain,
            line,
            module: module.into(),
            msg: Some(format!(
                "block ({}) has unexpected author {}:\n{}",
                hex::encode(hsh),
                hex::encode(author),
                expect
            )),
        }
    }

    pub fn external(line: u32, module: &str, msg: String) -> Self {
        Self {
            source: None,
            kind: ErrorKind::Extern,
            line,
            module: module.into(),
            msg: Some(msg),
        }
    }
}

impl std::error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(src) = &self.source {
            writeln!(f, "{src}")?;
        }
        if let Some(msg) = &self.msg {
            write!(f, "{:?} {} {}: {}", self.kind, self.module, self.line, msg)
        } else {
            write!(f, "{:?} {} {}", self.kind, self.module, self.line)
        }
    }
}
