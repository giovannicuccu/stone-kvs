pub mod crc32c;

use std::error::Error;
use std::fmt;
use std::fmt::{Display, Formatter};
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
    pub fn open(config: WalConfig) -> Result<Self, WalOpenError> {
        if !config.path.exists() {
            return Err({
                WalOpenError{ path: Box::new(config.path), kind: WalOpenErrorKind::ConfigPathIsNotReadable}});
        }
        
        let wal_dir = config.path.join("wal");
        match  std::fs::create_dir_all(&wal_dir).map_err(|e|
            WalOpenErrorKind::CannotCreateWalDirectory(e)) {
            Ok(..) => Ok(Self { config }),
            Err(kind) =>  Err(WalOpenError { path: Box::new(config.path), kind })
        }
    }
}

#[derive(Debug)]
pub struct WalOpenError {
    pub path: Box<PathBuf>,
    pub kind: WalOpenErrorKind,
}

impl Display for WalOpenError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "error reading `{}`", self.path.display())
    }
}

impl Error for WalOpenError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.kind {
            WalOpenErrorKind::CannotCreateWalDirectory(e) => Some(e),
            _ => None,
        }
    }
}


#[derive(Debug)]
pub enum WalOpenErrorKind {
    ConfigPathIsNotReadable,
    CannotCreateWalDirectory(std::io::Error),
}
