use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;
use crate::wal::crc32c::IncrementalCrc32c;
use crate::wal::wal_commons::*;

struct WriteRequest {
    key: Vec<u8>,
    value: Vec<u8>,
    response_tx: mpsc::SyncSender<Result<u64, WalError>>,
}

#[derive(Debug)]
pub struct ChannelWal {
    sender: mpsc::SyncSender<WriteRequest>,
    _writer_handle: thread::JoinHandle<()>,
    sequence:Arc<AtomicU64>,
    config: WalConfig,
}

impl ChannelWal {
    pub fn new(config: WalConfig) -> Result<Self, WalError> {
        let inner_config = config.clone();
        let (sender, receiver) = mpsc::sync_channel(config.buffer_size);
        let sequence = Arc::new(AtomicU64::new(0));
        let sequence_clone = sequence.clone();
        let writer_handle = thread::spawn(move || {
            let wal = match InnerWal::open(config.clone(),sequence_clone) {
                Ok(wal) => wal,
                Err(_) => return,
            };
            write_entries(receiver, wal);
        });

        Ok(Self {
            sender,
            _writer_handle: writer_handle,
            sequence,
            config: inner_config,
        })
    }
}

fn write_entries(receiver: Receiver<WriteRequest>, mut wal: InnerWal) {
    let timeout_millis = Duration::from_millis(2);
    let mut counter=0;

    loop {
        let mut response_holder=vec![];
        while counter < 10 && let Ok(WriteRequest { key, value, response_tx }) = receiver.recv_timeout(timeout_millis) {
            let result = wal.write_entry(&key, &value);
            counter = counter + 1;
            response_holder.push((response_tx, result));
            //let _ = response_tx.send(result); // Ignore if receiver dropped
        }
        for (response_tx, result) in response_holder {
            let _ = response_tx.send(result);
        }
        let _ = wal.commit();
        counter=0;
    }

}

impl super::Wal for ChannelWal {
    fn write_entry(&self, key: &[u8], value: &[u8]) -> Result<u64, WalError> {
        let (response_tx, response_rx) = mpsc::sync_channel(1);

        // Send request to writer thread
        self.sender.send(WriteRequest {
            key: key.to_vec(),
            value: value.to_vec(),
            response_tx,
        }).map_err(|_| WalError {
            path: PathBuf::new(),
            kind: WalErrorKind::WriterThreadDisconnected,
        })?;

        // Wait for response
        response_rx.recv()
            .map_err(|_| WalError {
                path: PathBuf::new(),
                kind: WalErrorKind::WriterThreadDisconnected,
            })?
    }

    fn entries(&self) -> Result<WalEntryIterator, WalError> {
        let wal_log_path = self.config.path.join("wal").join("wal.log");
        WalEntryIterator::new(wal_log_path)
    }

    fn sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }
}

pub struct InnerWal {
    config: WalConfig,
    sequence:Arc<AtomicU64>,
    file: BufWriter<File>,
    wal_log_path: PathBuf,
}

impl InnerWal {
    pub fn open(config: WalConfig, sequence: Arc<AtomicU64>) -> Result<Self, WalError> {
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

        Ok(Self { config,  sequence, file: BufWriter::with_capacity(4096,file) , wal_log_path })
    }

    pub fn sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }

    pub fn log_path(&self) -> &PathBuf {
        &self.wal_log_path
    }

    fn write_entry(&mut self, key: &[u8], value: &[u8]) -> Result<u64, WalError> {
        let sequence=self.sequence.fetch_add(1, Ordering::SeqCst)+1;
        let sequence_bytes = sequence.to_le_bytes();

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

        self.file.write_all(&crc32c_value.to_le_bytes()).map_err(map_err)?;
        self.file.write_all(&sequence_bytes).map_err(map_err)?;
        self.file.write(&[PUT_OPERATION]).map_err(map_err)?;
        self.file.write_all(&key_size.to_le_bytes()).map_err(map_err)?;
        self.file.write_all(&value_size.to_le_bytes()).map_err(map_err)?;
        self.file.write_all(key).map_err(map_err)?;
        self.file.write_all(value).map_err(map_err)?;
        //self.file.flush().map_err(map_err)?;
        //self.file.sync_all().map_err(map_err)?;

        Ok(sequence)
    }

    fn commit(&mut self) -> Result<(), WalError> {
        let map_err = |e| map_io_error(&self.wal_log_path, e);
        self.file.flush().map_err(map_err)?;
        //self.file.get_mut().sync_all().map_err(map_err)?;
        //self.file.sync_all().map_err(map_err)?;

        Ok(())
    }
}
