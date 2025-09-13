pub mod crc32c;

use std::error::Error;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::io::{self, Write, Read, Seek, SeekFrom};
use std::fs::{File, OpenOptions};

const PUT_OPERATION: u8 = 1;

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
pub struct WalEntry {
    pub crc32c: u32,
    pub sequence: u64,
    pub entry_type: u8,
    pub key_size: u32,
    pub value_size: u32,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    crc_data: Vec<u8>,
}

impl WalEntry {
    fn new(crc32c: u32, sequence: u64, entry_type: u8, key_size: u32, value_size: u32, key: Vec<u8>, value: Vec<u8>) -> Self {
        let mut crc_data = Vec::new();
        crc_data.extend_from_slice(&sequence.to_le_bytes());
        crc_data.push(entry_type);
        crc_data.extend_from_slice(&key_size.to_le_bytes());
        crc_data.extend_from_slice(&value_size.to_le_bytes());
        crc_data.extend_from_slice(&key);
        crc_data.extend_from_slice(&value);
        
        Self {
            crc32c,
            sequence,
            entry_type,
            key_size,
            value_size,
            key,
            value,
            crc_data,
        }
    }

    pub fn data_for_crc(&self) -> &[u8] {
        &self.crc_data
    }
}

#[derive(Debug)]
pub struct Wal {
    config: WalConfig,
    sequence: u64,
}

impl Wal {
    pub fn open(config: WalConfig) -> Result<Self, WalOpenError> {
        if !config.path.exists() {
            return Err(WalOpenError {
                path: Box::new(config.path.clone()),
                kind: WalOpenErrorKind::ConfigPathIsNotReadable,
            });
        }

        let wal_dir = config.path.join("wal");
        std::fs::create_dir_all(&wal_dir)
            .map_err(|e| WalOpenError {
                path: Box::new(config.path.clone()),
                kind: WalOpenErrorKind::CannotCreateWalDirectory(e),
            })?;

        Ok(Self { config, sequence: 0 })
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn write_entry(&mut self, key: &[u8], value: &[u8]) -> Result<u64, io::Error> {
        self.sequence += 1;
        
        let wal_log_path = self.config.path.join("wal").join("wal.log");
        
        // Create file if it doesn't exist and write header
        if !wal_log_path.exists() {
            let mut file = File::create(&wal_log_path)?;
            // File Header: [Magic(4B) | Version(4B) | Reserved(8B)]
            file.write_all(b"WAL\0")?; // Magic number
            file.write_all(&1u32.to_le_bytes())?; // Version 1
            file.write_all(&[0u8; 8])?; // Reserved bytes
            file.flush()?;
        }
        
        // Append the entry to the file
        let mut file = OpenOptions::new().append(true).open(&wal_log_path)?;
        
        // Record Format: [CRC32C(4B) | Sequence(8B) | Type(1B) | Key_Size(4B) | Value_Size(4B) | Key | Value]
        let key_size = key.len() as u32;
        let value_size = value.len() as u32;
        
        // Build data portion for CRC calculation
        let mut crc_data = Vec::new();
        crc_data.extend_from_slice(&self.sequence.to_le_bytes());
        crc_data.push(PUT_OPERATION);
        crc_data.extend_from_slice(&key_size.to_le_bytes());
        crc_data.extend_from_slice(&value_size.to_le_bytes());
        crc_data.extend_from_slice(key);
        crc_data.extend_from_slice(value);
        
        let crc32c_value = crate::wal::crc32c::crc32c(&crc_data);
        
        // Write the record with 2 operations: CRC + data
        file.write_all(&crc32c_value.to_le_bytes())?;
        file.write_all(&crc_data)?;
        file.flush()?;
        
        Ok(self.sequence)
    }

    pub fn entries(&self) -> WalEntryIterator {
        let wal_log_path = self.config.path.join("wal").join("wal.log");
        WalEntryIterator::new(wal_log_path)
    }
}

pub struct WalEntryIterator {
    file: Option<File>,
    finished: bool,
}

impl WalEntryIterator {
    fn new(wal_log_path: PathBuf) -> Self {
        let file = match File::open(wal_log_path) {
            Ok(mut f) => {
                // Skip file header immediately (16 bytes)
                let mut header = [0u8; 16];
                if f.read_exact(&mut header).is_ok() {
                    Some(f)
                } else {
                    None // Invalid file or empty
                }
            }
            Err(_) => None,
        };
        
        Self { 
            file,
            finished: false,
        }
    }
}

impl Iterator for WalEntryIterator {
    type Item = Result<WalEntry, io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        
        let file = match self.file.as_mut() {
            Some(f) => f,
            None => {
                self.finished = true;
                return None; // File doesn't exist, return empty
            }
        };

        // Read record header: [CRC32C(4B) | Sequence(8B) | Type(1B) | Key_Size(4B) | Value_Size(4B)] = 21 bytes
        let mut header = [0u8; 21];
        if file.read_exact(&mut header).is_err() {
            self.finished = true;
            return None;
        }

        // Decode header fields using array slicing
        let crc32c = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let sequence = u64::from_le_bytes([header[4], header[5], header[6], header[7], header[8], header[9], header[10], header[11]]);
        let entry_type = header[12];
        let key_size = u32::from_le_bytes([header[13], header[14], header[15], header[16]]);
        let value_size = u32::from_le_bytes([header[17], header[18], header[19], header[20]]);

        // Read key and value in one operation
        let total_data_size = (key_size + value_size) as usize;
        let mut key_value_data = vec![0u8; total_data_size];
        if file.read_exact(&mut key_value_data).is_err() {
            return Some(Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Invalid record")));
        }

        // Split key and value
        let key = key_value_data[..key_size as usize].to_vec();
        let value = key_value_data[key_size as usize..].to_vec();

        Some(Ok(WalEntry::new(
            crc32c,
            sequence,
            entry_type,
            key_size,
            value_size,
            key,
            value,
        )))
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
