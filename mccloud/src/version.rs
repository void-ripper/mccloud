use std::fmt::Display;

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(BorshDeserialize, BorshSerialize, Clone)]
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
            branch: env!("BRANCH").into(),
            commit: env!("COMMIT").into(),
        }
    }
}

impl Eq for Version {}

impl PartialEq for Version {
    fn eq(&self, o: &Self) -> bool {
        self.major == o.major && self.minor == o.minor && self.patch == o.patch
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "v{}.{}.{} {} {} {}",
            self.major, self.minor, self.patch, self.branch, self.commit, self.target
        )
    }
}
