pub mod crc32c;

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct WalConfig {
    pub path: PathBuf,
}

impl WalConfig {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

pub struct Wal {
    config: WalConfig,
}

impl Wal {
    pub fn open(config: WalConfig) -> Result<Self, WalError> {
        Ok(Self { config })
    }
}

#[derive(Debug)]
pub enum WalError {
    Io(std::io::Error),
    InvalidConfig(String),
}

impl From<std::io::Error> for WalError {
    fn from(err: std::io::Error) -> Self {
        WalError::Io(err)
    }
}
