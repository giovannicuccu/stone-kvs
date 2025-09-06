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

#[derive(Debug)]
pub struct Wal {
    config: WalConfig,
}

impl Wal {
    pub fn open(config: WalConfig) -> Result<Self, WalError> {
        if !config.path.exists() {
            return Err(WalError::InvalidConfig(format!(
                "Directory does not exist: {}, check your configuration/file permissions.",
                config.path.display()
            )));
        }
        
        let wal_dir = config.path.join("wal");
        std::fs::create_dir_all(&wal_dir).map_err(|e| {
            WalError::InvalidConfig(format!(
                "Failed to create WAL directory {}: {}",
                wal_dir.display(),
                e
            ))
        })?;
        
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
