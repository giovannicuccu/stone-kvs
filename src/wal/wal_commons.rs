use crate::wal::channel_wal::WriteRequest;
use crate::wal::crc32c::IncrementalCrc32c;
use std::error::Error;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::path::PathBuf;
use std::sync::mpsc::SendError;

pub(crate) const PUT_OPERATION: u8 = 1;
pub(crate) const WAL_MAGIC: &[u8; 4] = b"WAL\0";
pub(crate) const WAL_VERSION: u32 = 1;
pub(crate) const WAL_ENTRY_HEADER_LEN: usize = 21;
pub(crate) const WAL_FILE_HEADER_LEN: usize = 16;

/// Helper function to convert io::Error to WalError for file operations
pub(crate) fn map_io_error(path: String, e: io::Error) -> WalError {
    WalError {
        storage_description: path,
        kind: WalErrorKind::WalFileError(e),
    }
}

pub trait IWalConfig: Clone + Send {
    type Storage: Write + Read + Seek;

    fn create_storage(&self) -> Result<Self::Storage, WalError>;
    fn buffer_size(&self) -> usize;

    fn storage_description(&self) -> String;
}

#[derive(Debug, Clone)]
pub struct WalConfig {
    pub path: PathBuf,
    buffer_size: usize,
}

impl WalConfig {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            buffer_size: 100, // Default buffer size
        }
    }

    pub fn with_buffer_size(mut self, buffer_size: usize) -> Self {
        self.buffer_size = buffer_size;
        self
    }
}

impl IWalConfig for WalConfig {
    type Storage = File;

    fn create_storage(&self) -> Result<Self::Storage, WalError> {
        if !self.path.exists() {
            return Err(WalError {
                storage_description: self.storage_description(),
                kind: WalErrorKind::ConfigPathIsNotReadable,
            });
        }

        let wal_dir = self.path.join("wal");
        std::fs::create_dir_all(&wal_dir).map_err(|e| WalError {
            storage_description: self.storage_description(),
            kind: WalErrorKind::CannotCreateWalDirectory(e),
        })?;

        let wal_log_path = wal_dir.join("wal.log");

        // Create or open the WAL file
        let file = if wal_log_path.exists() {
            // Open existing file in append mode
            println!("file opened {}", wal_log_path.as_os_str().to_str().unwrap());
            OpenOptions::new()
                .append(true)
                .open(&wal_log_path)
                .map_err(|e| WalError {
                    storage_description: self.storage_description(),
                    kind: WalErrorKind::WalFileError(e),
                })?
        } else {
            let mut file = File::create(&wal_log_path).map_err(|e| WalError {
                storage_description: self.storage_description(),
                kind: WalErrorKind::WalFileError(e),
            })?;
            //file.flush();
            //file.sync_all();
            file
        };
        Ok(file)
    }

    fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    fn storage_description(&self) -> String {
        self.path.display().to_string()
    }
}

#[derive(Debug)]
pub struct WalEntry {
    pub crc32c: u32,
    pub sequence: u64,
    pub entry_type: u8,
    pub key_size: u32,
    pub value_size: u32,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

impl WalEntry {
    pub(crate) fn new(
        crc32c: u32,
        sequence: u64,
        entry_type: u8,
        key_size: u32,
        value_size: u32,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Self {
        Self {
            crc32c,
            sequence,
            entry_type,
            key_size,
            value_size,
            key,
            value,
        }
    }
}

#[derive(Debug)]
pub struct WalError {
    pub storage_description: String,
    pub kind: WalErrorKind,
}

impl Display for WalError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "error reading `{}` {:?}",
            self.storage_description, self.kind
        )
    }
}

impl Error for WalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.kind {
            WalErrorKind::CannotCreateWalDirectory(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum WalErrorKind {
    ConfigPathIsNotReadable,
    CannotCreateWalDirectory(io::Error),
    WalFileCorrupted(io::Error),
    WalFileError(io::Error),
    WalFileDoesntExist,
    ChannelDisconnected(SendError<WriteRequest>),
    WriterThreadDisconnected,
}
