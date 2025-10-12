use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::RwLock;
use crate::wal::crc32c::IncrementalCrc32c;
use crate::wal::wal_commons::*;

pub(crate) struct WalStatus {
    sequence: u64,
    file: File,
}

pub struct SyncWal {
    config: WalConfig,
    wal_status: RwLock<WalStatus>,
    wal_log_path: PathBuf,
}

impl SyncWal {
    pub fn open(config: WalConfig) -> Result<Self, WalError> {
        if !config.path.exists() {
            return Err(WalError {
                path: config.path.clone(),
                kind: WalErrorKind::ConfigPathIsNotReadable,
            });
        }

        let wal_dir = config.path.join("wal");
        std::fs::create_dir_all(&wal_dir)
            .map_err(|e| WalError {
                path: config.path.clone(),
                kind: WalErrorKind::CannotCreateWalDirectory(e),
            })?;

        let wal_log_path = wal_dir.join("wal.log");

        // Create or open the WAL file
        let file = if wal_log_path.exists() {
            // Open existing file in append mode
            OpenOptions::new()
                .append(true)
                .open(&wal_log_path)
                .map_err(|e| WalError {
                    path: config.path.clone(),
                    kind: WalErrorKind::WalFileError(e),
                })?
        } else {
            // Create new file and write header
            let mut file = File::create(&wal_log_path)
                .map_err(|e| WalError {
                    path: config.path.clone(),
                    kind: WalErrorKind::WalFileError(e),
                })?;

            // Write WAL file header: [Magic(4B) | Version(4B) | Reserved(8B)]
            file.write_all(WAL_MAGIC)
                .and_then(|_| file.write_all(&WAL_VERSION.to_le_bytes()))
                .and_then(|_| file.write_all(&[0u8; 8]))
                .and_then(|_| file.flush())
                .map_err(|e| WalError {
                    path: config.path.clone(),
                    kind: WalErrorKind::WalFileError(e),
                })?;
            file
        };

        Ok(Self { config, wal_status: RwLock::new(WalStatus { sequence: 0, file }), wal_log_path })
    }

    pub fn sequence(&self) -> u64 {
        self.wal_status.read().unwrap().sequence
    }

    pub fn log_path(&self) -> &PathBuf {
        &self.wal_log_path
    }
}

impl super::Wal for SyncWal {
    fn write_entry(&self, key: &[u8], value: &[u8]) -> Result<u64, WalError> {
        let mut status = self.wal_status.write().unwrap();
        status.sequence += 1;
        let sequence_bytes = status.sequence.to_le_bytes();

        // Helper to convert io::Error to WalError
        let map_err = |e| map_io_error(&self.wal_log_path, e);

        // Record Format: [CRC32C(4B) | Sequence(8B) | Type(1B) | Key_Size(4B) | Value_Size(4B) | Key | Value]
        let key_size = key.len() as u32;
        let value_size = value.len() as u32;

        let mut crc32c = IncrementalCrc32c::new();
        crc32c.update(&sequence_bytes);
        crc32c.update(&[PUT_OPERATION]);
        crc32c.update(&key_size.to_le_bytes());
        crc32c.update(&value_size.to_le_bytes());
        crc32c.update(key);
        crc32c.update(value);
        let crc32c_value = crc32c.finalize();

        status.file.write_all(&crc32c_value.to_le_bytes()).map_err(map_err)?;
        status.file.write_all(&sequence_bytes).map_err(map_err)?;
        status.file.write(&[PUT_OPERATION]).map_err(map_err)?;
        status.file.write_all(&key_size.to_le_bytes()).map_err(map_err)?;
        status.file.write_all(&value_size.to_le_bytes()).map_err(map_err)?;
        status.file.write_all(key).map_err(map_err)?;
        status.file.write_all(value).map_err(map_err)?;
        status.file.flush().map_err(map_err)?;

        Ok(status.sequence)
    }

    fn entries(&self) -> Result<WalEntryIterator, WalError> {
        let wal_log_path = self.config.path.join("wal").join("wal.log");
        WalEntryIterator::new(wal_log_path)
    }
}
