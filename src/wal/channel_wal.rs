use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use crate::wal::sync_wal::SyncWal;
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
    config: WalConfig,
}

impl ChannelWal {
    pub fn new(config: WalConfig) -> Result<Self, WalError> {
        let inner_config = config.clone();
        let (sender, receiver) = mpsc::sync_channel(config.buffer_size);

        let writer_handle = thread::spawn(move || {
            let wal = match SyncWal::open(config.clone()) {
                Ok(wal) => wal,
                Err(_) => return,
            };

            while let Ok(WriteRequest { key, value, response_tx }) = receiver.recv() {
                let result = <SyncWal as super::Wal>::write_entry(&wal, &key, &value);
                let _ = response_tx.send(result); // Ignore if receiver dropped
            }
        });

        Ok(Self {
            sender,
            _writer_handle: writer_handle,
            config: inner_config,
        })
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
}
