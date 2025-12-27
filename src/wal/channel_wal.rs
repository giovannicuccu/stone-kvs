use crate::wal::crc32c::IncrementalCrc32c;
use crate::wal::wal_commons::*;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, mpsc};
use std::time::Duration;
use std::{io, mem, thread};

pub const PUT_OPERATION: u8 = 1;
pub const BLOCK_SIZE: usize = 32768; //1024 * 32
const WAL_CHUNK_HEADER_SIZE: usize = 7;
const WAL_RECORD_HEADER_SIZE: usize = 17;
const WAL_RECORD_HEADER_OPERATION_IDX: usize = 8;
const WAL_RECORD_HEADER_VALUE_IDX: usize = 13;

const CHUNK_TYPE_FULL: u8 = 1;
const CHUNK_TYPE_FIRST: u8 = 2;
const CHUNK_TYPE_MIDDLE: u8 = 3;
const CHUNK_TYPE_LAST: u8 = 4;

const DURATION_MICROS: u64 = 1;
const BATCH_SIZE: usize = 64;

pub struct WriteRequest {
    key: Vec<u8>,
    value: Vec<u8>,
    response_tx: mpsc::SyncSender<Result<(u64, Vec<u8>, Vec<u8>), WalError>>,
}

#[derive(Debug)]
pub struct ChannelWal<C: IWalConfig> {
    sender: mpsc::SyncSender<WriteRequest>,
    _writer_handle: thread::JoinHandle<()>,
    sequence: Arc<AtomicU64>,
    config: C,
}

impl<C: IWalConfig + 'static> ChannelWal<C> {
    pub fn open(config: C) -> Result<Self, WalError> {
        let inner_config = config.clone();
        let (sender, receiver) = mpsc::sync_channel(config.buffer_size());
        let sequence = Arc::new(AtomicU64::new(0));
        let sequence_clone = sequence.clone();
        //create the wal synchronously so we can crate the file and allow empty iteration

        let config_clone = config.clone();
        let writer_handle = thread::spawn(move || {
            let wal = InnerWal::open(config_clone, sequence_clone).unwrap();
            //if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            write_entries(receiver, wal);
            /*})) {
                let panic_msg = if let Some(s) = e.downcast_ref::<&str>() {
                    format!("WAL writer thread panicked!\nPanic message: {}", s)
                } else if let Some(s) = e.downcast_ref::<String>() {
                    format!("WAL writer thread panicked!\nPanic message: {}", s)
                } else {
                    "WAL writer thread panicked!\nPanic message: <unknown>".to_string()
                };
                error!("Panic caught: {}", panic_msg);
            }*/
        });

        Ok(Self {
            sender,
            _writer_handle: writer_handle,
            sequence,
            config: inner_config,
        })
    }

    pub fn write_entry(
        &self,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Result<(u64, Vec<u8>, Vec<u8>), WalError> {
        let (response_tx, response_rx) = mpsc::sync_channel(1);

        // Send request to writer thread
        self.sender
            .send(WriteRequest {
                key,
                value,
                response_tx,
            })
            .map_err(|err| WalError {
                storage_description: "".to_string(),
                kind: WalErrorKind::ChannelDisconnected(err),
            })?;

        // Wait for response
        response_rx.recv().map_err(|_| WalError {
            storage_description: "".to_string(),
            kind: WalErrorKind::WriterThreadDisconnected,
        })?
    }

    pub fn entries(&self) -> Result<ChannelWalEntryIterator<C>, WalError> {
        ChannelWalEntryIterator::new(self.config.clone())
    }

    pub fn sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }
}

fn write_entries<C: IWalConfig>(receiver: Receiver<WriteRequest>, mut wal: InnerWal<C>) {
    let timeout_millis = Duration::from_micros(DURATION_MICROS);
    let mut counter = 0;

    loop {
        let mut response_holder = vec![];
        while counter < BATCH_SIZE
            && let Ok(WriteRequest {
                key,
                value,
                response_tx,
            }) = receiver.recv_timeout(timeout_millis)
        {
            log::info!("writing_entry to wal");
            let result = wal.write_entry(key, value);
            counter = counter + 1;
            response_holder.push((response_tx, result));
        }
        if !response_holder.is_empty() {
            let _ = wal.commit();
        }
        for (response_tx, result) in response_holder {
            let _ = response_tx.send(result);
        }
        counter = 0;
    }
}
///
/// This is the wal struct responsible for writing entries to disk in the
/// rocksdb block format
/// there are three main concepts
/// the entry (key/value) being added
/// the block i.e. the data that is written to the disk
/// the chunks i.e. the parts of the entry that fit in a block
/// an entry can span one single chunk if its size is smaller than the block (residual) capacity
/// an entry can span multiple block if its size exceeds the block capacity in this case the entry is split in multiple chunks
/// more than one entry can be written in a block it their size is less than the block size
///
/// Some invariants:
///
/// Every chunk have a checksum
/// When starting a new chunk the checksum is zero
/// A chunk must be written to a block with its checksum
/// A chunk cannot be allocated in a block it the (block) residual capacity is less than the chunk header size
///
/// An entry must span one or more chunks
///
/// A block must contain at least one chunk
/// Only the whole block is written to the disk, it's the atom of data saved to the storage
///
pub struct InnerWal<C: IWalConfig> {
    config: C,
    sequence: Arc<AtomicU64>,
    file: C::Storage,
    block_buffer: Box<[u8; BLOCK_SIZE]>, // Safe, fast, idiomatic
    block_offset: usize,
    crc32c: IncrementalCrc32c,
}

impl<C: IWalConfig> InnerWal<C> {
    pub fn open(config: C, sequence: Arc<AtomicU64>) -> Result<Self, WalError> {
        let storage = config.create_storage()?;
        Ok(Self {
            config,
            sequence,
            file: storage,
            block_buffer: Box::new([0u8; BLOCK_SIZE]),
            block_offset: 0,
            crc32c: IncrementalCrc32c::new(),
        })
    }

    pub fn sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }

    pub fn write_entry(
        &mut self,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Result<(u64, Vec<u8>, Vec<u8>), WalError> {
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst) + 1;

        // Helper to convert io::Error to WalError
        //let map_err = |e| map_io_error(&self.wal_log_path, e);

        let header_data = self.build_entry_header(
            sequence,
            PUT_OPERATION,
            key.len() as u32,
            value.len() as u32,
        );

        let mut bytes_to_write = header_data.len() + key.len() + value.len();

        let block_type: u8 = if bytes_to_write >= (BLOCK_SIZE - self.block_offset) {
            CHUNK_TYPE_FIRST
        } else {
            CHUNK_TYPE_FULL
        };

        /*
        INVARIANT
        BLOCK_SIZE - (self.block_offset + WAL_CHUNK_HEADER_SIZE)>0
        i.e. I can write the chunk header plus at least one byte of data
        the invariant initially holds self.block_offset=0
         */
        let chunk_len =
            bytes_to_write.min(BLOCK_SIZE - (self.block_offset + WAL_CHUNK_HEADER_SIZE));
        let mut crc_offset = self.block_offset;
        self.write_chunk_header(chunk_len, block_type);
        (bytes_to_write, crc_offset) = self.write_data(&header_data, crc_offset, bytes_to_write);
        (bytes_to_write, crc_offset) = self.write_data(&key, crc_offset, bytes_to_write);
        (_, crc_offset) = self.write_data(&value, crc_offset, bytes_to_write);
        /*
        Restore the invariants (chunk size vs block size and crc) if needed
         */
        //checksum chunk invariant
        self.finalize_checksum(crc_offset);
        if (self.block_offset + WAL_CHUNK_HEADER_SIZE) >= BLOCK_SIZE {
            self.write_data_and_reset_block();
        }
        Ok((sequence, key, value))
    }

    fn build_entry_header(
        &self,
        sequence: u64,
        operation: u8,
        key_len: u32,
        value_len: u32,
    ) -> [u8; WAL_RECORD_HEADER_SIZE] {
        let mut header_data = [0u8; WAL_RECORD_HEADER_SIZE];
        header_data[0..WAL_RECORD_HEADER_OPERATION_IDX].copy_from_slice(&sequence.to_le_bytes());
        header_data[WAL_RECORD_HEADER_OPERATION_IDX] = operation;
        header_data[WAL_RECORD_HEADER_OPERATION_IDX + 1..WAL_RECORD_HEADER_VALUE_IDX]
            .copy_from_slice(&key_len.to_le_bytes());
        header_data[WAL_RECORD_HEADER_VALUE_IDX..WAL_RECORD_HEADER_SIZE]
            .copy_from_slice(&value_len.to_le_bytes());
        header_data
    }

    fn write_chunk_header(&mut self, chunk_len: usize, block_type: u8) {
        self.block_buffer[self.block_offset + 4..self.block_offset + 6]
            .copy_from_slice(&(chunk_len as u16).to_le_bytes());
        self.block_buffer[self.block_offset + 6] = block_type;
        self.block_offset += WAL_CHUNK_HEADER_SIZE;
    }
    fn write_data(
        &mut self,
        data: &[u8],
        mut crc_offset: usize,
        mut bytes_to_write: usize,
    ) -> (usize, usize) {
        let mut data_offset = 0;
        // writes the data to the buffer and on file if its len exceeds the available data in the block
        while data_offset < data.len() {
            /*
            INVARIANT
            BLOCK_SIZE - self.block_offset > 0
             */
            let remaining = BLOCK_SIZE - self.block_offset;
            if remaining > 0 {
                let to_copy = (data.len() - data_offset).min(remaining);
                self.block_buffer[self.block_offset..self.block_offset + to_copy]
                    .copy_from_slice(&data[data_offset..data_offset + to_copy]);
                self.crc32c
                    .update(&data[data_offset..data_offset + to_copy]);
                self.block_offset += to_copy;
                data_offset += to_copy;
                bytes_to_write -= to_copy;
            } else {
                //Keep the invariant if needed
                self.write_data_and_re_init_chunk(crc_offset, bytes_to_write);
                crc_offset = 0;
            }
        }
        (bytes_to_write, crc_offset)
    }

    fn write_data_and_re_init_chunk(&mut self, crc_offset: usize, bytes_to_write: usize) {
        //checksum chunk invariant
        self.finalize_checksum(crc_offset);
        self.write_data_and_reset_block();
        let block_type = if bytes_to_write >= (BLOCK_SIZE - WAL_CHUNK_HEADER_SIZE) {
            CHUNK_TYPE_MIDDLE
        } else {
            CHUNK_TYPE_LAST
        };
        let record_len =
            bytes_to_write.min(BLOCK_SIZE - (self.block_offset + WAL_CHUNK_HEADER_SIZE));
        self.write_chunk_header(record_len, block_type);
    }

    fn write_data_and_reset_block(&mut self) {
        self.file.write_all(self.block_buffer.as_ref());
        self.block_buffer.fill(0u8);
        self.block_offset = 0;
    }

    fn finalize_checksum(&mut self, crc_offset: usize) {
        let checksum = self.crc32c.value();
        self.crc32c.reset();
        self.block_buffer[crc_offset..crc_offset + 4].copy_from_slice(&checksum.to_le_bytes());
    }

    fn commit(&mut self) -> Result<(), WalError> {
        //println!("committing wal");
        let map_err = |e| map_io_error(self.config.storage_description(), e);
        self.file
            .write_all(self.block_buffer.as_ref())
            .map_err(map_err)?;
        self.file.flush().map_err(map_err)?;
        //self.file.get_mut().sync_all().map_err(map_err)?;
        self.file
            .seek(SeekFrom::Current(-(BLOCK_SIZE as i64)))
            .map_err(map_err)?;

        Ok(())
    }
}

pub struct ChunkStatus {
    chunk_crc: u32,
    chunk_len: u16,
    chunk_type: u8,
    chunk_offset: u16,
}

pub struct ChannelWalEntryIterator<C: IWalConfig> {
    wal_config: C,
    storage: C::Storage,
    finished: bool,
    block_buffer: Box<[u8; BLOCK_SIZE]>, // Safe, fast, idiomatic
    block_offset: usize,
}

impl<C: IWalConfig> ChannelWalEntryIterator<C> {
    pub(crate) fn new(wal_config: C) -> Result<Self, WalError> {
        Self::create_from_storage(wal_config)
    }

    fn create_from_storage(wal_config: C) -> Result<Self, WalError> {
        let mut block_buffer = Box::new([0u8; BLOCK_SIZE]);
        let mut storage = wal_config.create_storage()?;
        let _ = storage.read_exact(block_buffer.as_mut());
        Ok(Self {
            wal_config,
            storage,
            finished: false,
            block_buffer,
            block_offset: 0,
        })
    }
    fn read_data(
        &mut self,
        data_len: usize,
        chunk_status: &mut ChunkStatus,
    ) -> Result<Vec<u8>, WalError> {
        let mut data = vec![0; data_len];
        let mut data_read = 0;
        while data_read < data_len {
            if (chunk_status.chunk_type == 1 || chunk_status.chunk_type == 4)
                && ((chunk_status.chunk_offset as usize) + (data_len - data_read))
                    > (chunk_status.chunk_len as usize)
            {
                return Err(WalError {
                    storage_description: self.wal_config.storage_description(),
                    kind: WalErrorKind::WalFileCorrupted(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Data corruption: reading beyond wal entry end",
                    )),
                });
            }
            let remaining = (chunk_status.chunk_len - chunk_status.chunk_offset) as usize;
            if remaining > 0 {
                let to_copy = (data_len - data_read).min(remaining);
                data[data_read..data_read + to_copy].copy_from_slice(
                    &self.block_buffer[self.block_offset..self.block_offset + to_copy],
                );
                self.block_offset += to_copy;
                chunk_status.chunk_offset += to_copy as u16;
                data_read += to_copy;
            }

            if BLOCK_SIZE - self.block_offset == 0 {
                let _ = self.storage.read_exact(self.block_buffer.as_mut());
                let (new_chunk_status, block_offset) = read_chunk_header(&self.block_buffer, 0);
                let _ = mem::replace(chunk_status, new_chunk_status);
                self.block_offset = block_offset
            }
        }
        Ok(data)
    }

    fn read_entry_header(&self, header_data: Vec<u8>) -> (u64, u8, u32, u32) {
        let entry_sequence = u64::from_le_bytes(header_data[0..8].try_into().unwrap());
        let operation = header_data[8];
        let key_len = u32::from_le_bytes(header_data[9..13].try_into().unwrap());
        let value_len = u32::from_le_bytes(header_data[13..17].try_into().unwrap());
        (entry_sequence, operation, key_len, value_len)
    }
}

fn read_chunk_header(
    block_buffer: &Box<[u8; BLOCK_SIZE]>,
    block_offset: usize,
) -> (ChunkStatus, usize) {
    let chunk_crc = u32::from_le_bytes(
        block_buffer[block_offset..block_offset + 4]
            .try_into()
            .unwrap(),
    );
    let chunk_len = u16::from_le_bytes(
        block_buffer[block_offset + 4..block_offset + 6]
            .try_into()
            .unwrap(),
    );
    let chunk_type = block_buffer[block_offset + 6];
    (
        ChunkStatus {
            chunk_crc,
            chunk_len,
            chunk_type,
            chunk_offset: 0,
        },
        block_offset + WAL_CHUNK_HEADER_SIZE,
    )
}

impl<C: IWalConfig> Iterator for ChannelWalEntryIterator<C> {
    type Item = Result<WalEntry, WalError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        /*
        INVARIANT
        BLOCK_SIZE - (self.block_offset + WAL_CHUNK_HEADER_SIZE)>0
        i.e. It's possible to read the chunk header at the current block_offset
        the invariant initially holds self.block_offset=0
         */
        let (mut chunk_status, block_offset) =
            read_chunk_header(&self.block_buffer, self.block_offset);

        self.block_offset = block_offset;
        if chunk_status.chunk_len == 0 {
            self.finished = true;
            return None;
        }
        let header_data = self
            .read_data(WAL_RECORD_HEADER_SIZE, &mut chunk_status)
            .unwrap();
        let (entry_sequence, operation, key_len, value_len) = self.read_entry_header(header_data);
        //println!("key_len: {} value_len {}", key_len, value_len);
        let key = self.read_data(key_len as usize, &mut chunk_status).unwrap();
        let value = self
            .read_data(value_len as usize, &mut chunk_status)
            .unwrap();
        /*
        restore the invariant if needed
         */
        if (self.block_offset + WAL_CHUNK_HEADER_SIZE) >= BLOCK_SIZE {
            let _ = self.storage.read_exact(self.block_buffer.as_mut());
            self.block_offset = 0;
        }
        Some(Ok(WalEntry::new(
            0,
            entry_sequence,
            operation,
            key_len,
            value_len,
            key,
            value,
        )))
    }
}
