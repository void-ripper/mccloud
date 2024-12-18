use borsh::{BorshDeserialize, BorshSerialize};

#[derive(BorshDeserialize, BorshSerialize)]
pub struct Version {
    major: u16,
    minor: u16,
    patch: u16,
    target: String,
    branch: String,
    commit: String,
}

impl Default for Version {
    fn default() -> Self {
        Self {
            major: env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap(),
            minor: env!("CARGO_PKG_VERSION_MINOR").parse().unwrap(),
            patch: env!("CARGO_PKG_VERSION_PATCH").parse().unwrap(),
            target: env!("TARGET").into(),
            branch: "".into(),
            commit: "".into(),
        }
    }
}
