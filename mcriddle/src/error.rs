use std::fmt::Display;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Copy, Clone)]
pub enum ErrorKind {
    Io,
    Addr,
    Sync,
    Encryption,
    Blockchain,
    Protocol,
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

    pub fn encrypt<E: Display>(line: u32, module: &str, e: E) -> Self {
        Self {
            source: None,
            kind: ErrorKind::Encryption,
            line,
            module: module.into(),
            msg: Some(e.to_string()),
        }
    }

    pub fn padding<E: Display>(line: u32, module: &str, e: E) -> Self {
        Self {
            source: None,
            kind: ErrorKind::Encryption,
            line,
            module: module.into(),
            msg: Some(e.to_string()),
        }
    }

    pub fn non_child_block(line: u32, module: &str, hsh: [u8; 32]) -> Self {
        Self {
            source: None,
            kind: ErrorKind::Blockchain,
            line,
            module: module.into(),
            msg: Some(format!("block ({}) is not child of current block", hex::encode(hsh))),
        }
    }
}

impl std::error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} {} {}: {:?}", self.kind, self.module, self.line, self.msg)
    }
}
