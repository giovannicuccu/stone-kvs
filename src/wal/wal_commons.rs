use crate::wal::channel_wal::WriteRequest;
use crate::wal::crc32c::IncrementalCrc32c;
use std::error::Error;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::mpsc::SendError;

pub(crate) const PUT_OPERATION: u8 = 1;
pub(crate) const WAL_MAGIC: &[u8; 4] = b"WAL\0";
pub(crate) const WAL_VERSION: u32 = 1;
pub(crate) const WAL_ENTRY_HEADER_LEN: usize = 21;
pub(crate) const WAL_FILE_HEADER_LEN: usize = 16;

/// Helper function to convert io::Error to WalError for file operations
pub(crate) fn map_io_error(path: &PathBuf, e: io::Error) -> WalError {
    WalError {
        path: path.clone(),
        kind: WalErrorKind::WalFileError(e),
    }
}

#[derive(Debug, Clone)]
pub struct WalConfig {
    pub path: PathBuf,
    pub buffer_size: usize,
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

pub struct WalEntryIterator {
    file: File,
    wal_log_path: PathBuf,
    finished: bool,
}

impl WalEntryIterator {
    pub(crate) fn new(wal_log_path: PathBuf) -> Result<Self, WalError> {
        if wal_log_path.exists() {
            let file = File::open(&wal_log_path).map_err(|e| WalError {
                path: wal_log_path.clone(),
                kind: WalErrorKind::WalFileError(e),
            })?;
            Self::create_from_existing_file(file, &wal_log_path)
        } else {
            Err(WalError {
                path: wal_log_path.clone(),
                kind: WalErrorKind::WalFileDoesntExist,
            })
        }
    }

    fn create_from_existing_file(mut file: File, wal_log_path: &PathBuf) -> Result<Self, WalError> {
        let mut header = [0u8; WAL_FILE_HEADER_LEN];
        file.read_exact(&mut header).map_err(|e| WalError {
            path: wal_log_path.clone(),
            kind: WalErrorKind::WalFileCorrupted(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Cannot read WAL header: {}", e),
            )),
        })?;

        Self::validate_header_file(&header).map_err(|msg| WalError {
            path: wal_log_path.clone(),
            kind: WalErrorKind::WalFileCorrupted(io::Error::new(io::ErrorKind::InvalidData, msg)),
        })?;

        Ok(Self {
            file,
            wal_log_path: wal_log_path.clone(),
            finished: false,
        })
    }

    fn validate_header_file(header: &[u8; WAL_FILE_HEADER_LEN]) -> Result<(), String> {
        let (magic, rest) = header.split_at(WAL_MAGIC.len());
        if magic != WAL_MAGIC {
            return Err("Invalid WAL file magic number".to_string());
        }

        let version = u32::from_le_bytes(rest[0..4].try_into().unwrap());
        if version != WAL_VERSION {
            return Err(format!(
                "Unsupported WAL version: {}, expected: {}",
                version, WAL_VERSION
            ));
        }
        Ok(())
    }
}

impl Iterator for WalEntryIterator {
    type Item = Result<WalEntry, WalError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        // Read record header: [CRC32C(4B) | Sequence(8B) | Type(1B) | Key_Size(4B) | Value_Size(4B)] = 21 bytes
        let mut header = [0u8; WAL_ENTRY_HEADER_LEN];
        if self.file.read_exact(&mut header).is_err() {
            self.finished = true;
            return None;
        }

        // Decode header fields using array slicing
        let crc32c = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let sequence = u64::from_le_bytes([
            header[4], header[5], header[6], header[7], header[8], header[9], header[10],
            header[11],
        ]);
        let entry_type = header[12];
        let key_size = u32::from_le_bytes([header[13], header[14], header[15], header[16]]);
        let value_size = u32::from_le_bytes([header[17], header[18], header[19], header[20]]);

        // Read key and value in one operation
        let total_data_size = (key_size + value_size) as usize;
        let mut key_value_data = vec![0u8; total_data_size];
        if self.file.read_exact(&mut key_value_data).is_err() {
            self.finished = true;
            return Some(Err(WalError {
                path: self.wal_log_path.clone(),
                kind: WalErrorKind::WalFileCorrupted(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid entry data key/value",
                )),
            }));
        }

        let mut crc32c_checksummer = IncrementalCrc32c::new();
        crc32c_checksummer.update(&header[4..]); //skip crc32c data in header
        crc32c_checksummer.update(&key_value_data);
        let computed_crc32c = crc32c_checksummer.finalize();
        if computed_crc32c != crc32c {
            self.finished = true;
            return Some(Err(WalError {
                path: self.wal_log_path.clone(),
                kind: WalErrorKind::WalFileCorrupted(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "CRC32 checksum mismatch",
                )),
            }));
        }

        let value = key_value_data.split_off(key_size as usize);
        let key = key_value_data;

        Some(Ok(WalEntry::new(
            crc32c, sequence, entry_type, key_size, value_size, key, value,
        )))
    }
}

#[derive(Debug)]
pub struct WalError {
    pub path: PathBuf,
    pub kind: WalErrorKind,
}

impl Display for WalError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "error reading `{}` {:?}", self.path.display(), self.kind)
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
