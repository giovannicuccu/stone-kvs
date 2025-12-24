use rand::Rng;
use std::fs::OpenOptions;
use std::io::{Seek, Write};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Barrier, Once};
use std::time::{Duration, Instant};
use std::{fs, thread};
use stone_kvs::wal::{ChannelWal, WalConfig, WalEntry};
use tempfile::TempDir;

const PUT_OPERATION: u8 = 1;
const BLOCK_SIZE: usize = 1024 * 32;
const WAL_CHUNK_HEADER_SIZE: usize = 7;
const WAL_RECORD_HEADER_SIZE: usize = 17;

static INIT: Once = Once::new();

fn init_test_logger() {
    INIT.call_once(|| {
        let log_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open("test_thread_macro.log")
            .expect("Failed to open log file");

        env_logger::Builder::new()
            .target(env_logger::Target::Pipe(Box::new(log_file)))
            .filter_level(log::LevelFilter::Debug)
            .init();
    });
}

#[test]
fn channel_write_entry_increments_sequence_number() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let wal = ChannelWal::open(config).unwrap();
    let initial_sequence = wal.sequence();

    let (entry_sequence, key, value) = wal
        .write_entry(b"key1".to_vec(), b"value1".to_vec())
        .unwrap();

    assert_eq!(entry_sequence, initial_sequence + 1);
    assert_eq!(wal.sequence(), initial_sequence + 1);
}

#[test]
fn channel_write_entry_can_be_read_back() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let wal = ChannelWal::open(config).unwrap();

    let key = b"test_key";
    let value = b"test_value";
    let (entry_sequence, _, _) = wal.write_entry(key.to_vec(), value.to_vec()).unwrap();

    let mut iter = wal.entries().unwrap();
    let entry = iter.next().unwrap().unwrap();

    assert_entry_matches(entry, entry_sequence, key, value);
}

#[test]
fn channel_write_entry_only_one_can_be_read_back() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let wal = ChannelWal::open(config).unwrap();

    let key = b"test_key";
    let value = b"test_value";
    let (entry_sequence, _, _) = wal.write_entry(key.to_vec(), value.to_vec()).unwrap();

    let mut iter = wal.entries().unwrap();
    let _ = iter.next().unwrap().unwrap();
    assert!(iter.next().is_none());
}

#[test]
fn channel_empty_no_one_can_be_read_back() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let mut wal = ChannelWal::open(config).unwrap();

    let mut iter = wal.entries().unwrap();
    assert!(iter.next().is_none());
}

#[test]
fn channel_write_two_entries_can_be_read_back() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let wal = ChannelWal::open(config).unwrap();

    let key = b"test_key";
    let value = b"test_value";
    let (_, _, _) = wal.write_entry(key.to_vec(), value.to_vec()).unwrap();

    let key = b"test_key2";
    let value = b"test_value2";
    let (entry_sequence, _, _) = wal.write_entry(key.to_vec(), value.to_vec()).unwrap();

    let mut iter = wal.entries().unwrap();
    let _ = iter.next().unwrap().unwrap();
    let entry = iter.next().unwrap().unwrap();

    assert_entry_matches(entry, entry_sequence, key, value);
}

#[test]
fn channel_write_one_entry_two_chunks() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let wal = ChannelWal::open(config).unwrap();

    let key = b"test_key";
    let value = vec![1 as u8; BLOCK_SIZE + 100];

    let (entry_sequence, _, value) = wal.write_entry(key.to_vec(), value).unwrap();

    let mut iter = wal.entries().unwrap();
    let entry = iter.next().unwrap().unwrap();

    assert_entry_matches(entry, entry_sequence, key, &value);
}

#[test]
fn channel_write_one_entry_in_first_chunk_and_one_across_two_blocks() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let wal = ChannelWal::open(config).unwrap();

    let key = b"test_key";
    let value = vec![1u8; BLOCK_SIZE / 2];

    let (_, _, _) = wal.write_entry(key.to_vec(), value).unwrap();

    let key = b"test_key_2";
    let value = vec![1u8; (BLOCK_SIZE / 2) + 200];

    let (entry_sequence, _, value) = wal.write_entry(key.to_vec(), value).unwrap();

    let mut iter = wal.entries().unwrap();
    let _ = iter.next().unwrap().unwrap();
    let entry = iter.next().unwrap().unwrap();

    assert_entry_matches(entry, entry_sequence, key, &value);
}

#[test]
fn channel_write_two_entry_in_first_block_and_one_across_two_blocks() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let wal = ChannelWal::open(config).unwrap();

    let key = b"test_key";
    let value = vec![1u8; BLOCK_SIZE / 4];

    let (_, _, _) = wal.write_entry(key.to_vec(), value).unwrap();

    let key = b"test_key_2";
    let value = vec![2u8; BLOCK_SIZE / 4];

    let (_, _, _) = wal.write_entry(key.to_vec(), value).unwrap();

    let key = b"test_key_3";
    let value = vec![3u8; (BLOCK_SIZE / 2) + 200];

    let (entry_sequence, _, value) = wal.write_entry(key.to_vec(), value).unwrap();

    let mut iter = wal.entries().unwrap();
    let _ = iter.next().unwrap().unwrap();
    let _ = iter.next().unwrap().unwrap();
    let entry = iter.next().unwrap().unwrap();
    println!("entry value len {}", entry.value.len());
    println!("value len {}", value.len());
    assert_entry_matches(entry, entry_sequence, key, &value);
}

#[test]
fn channel_write_one_entry_with_len_equal_to_block_size_minus_header_in_first_block_and_one_in_the_second_block()
 {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let wal = ChannelWal::open(config).unwrap();

    let key = b"test_key";
    let value = vec![
        1u8;
        BLOCK_SIZE
            - WAL_CHUNK_HEADER_SIZE
            - WAL_RECORD_HEADER_SIZE
            - key.len()
            - (WAL_CHUNK_HEADER_SIZE - 1)
    ];

    let (_, _, _) = wal.write_entry(key.to_vec(), value).unwrap();

    let key = b"test_key_2";
    let value = vec![1u8; (BLOCK_SIZE / 2) + 200];

    let (entry_sequence, _, value) = wal.write_entry(key.to_vec(), value).unwrap();

    let mut iter = wal.entries().unwrap();
    let _ = iter.next().unwrap().unwrap();
    let entry = iter.next().unwrap().unwrap();

    assert_entry_matches(entry, entry_sequence, key, &value);
}

#[test]
fn channel_write_from_two_threads() {
    init_test_logger();
    log::info!("channel_write_from_two_threads");
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let wal = Arc::new(ChannelWal::open(config).unwrap());

    let num_threads = 2;
    let mut handles = Vec::new();
    let start_barrier = Arc::new(Barrier::new(num_threads + 1));
    for thread_id in 0..num_threads {
        let db_clone = Arc::clone(&wal);
        let start_barrier_clone = Arc::clone(&start_barrier);
        let handle = thread::spawn(move || {
            start_barrier_clone.wait();
            log::info!("Thread {} started", thread_id);
            let mut rng = rand::thread_rng();
            let size = rng.gen_range(500..=1024);
            let data = vec![2 as u8; size];
            let key = format!("{}", thread_id);

            match db_clone.write_entry(key.as_bytes().to_vec(), data) {
                Ok((entry_sequence, _, value)) => {
                    log::info!(
                        "Thread {} wrote entry with sequence {}",
                        thread_id,
                        entry_sequence
                    );
                    //let mut iter = db_clone.entries().unwrap();
                    //let entry = iter.next().unwrap().unwrap();
                    //assert_entry_matches(entry, entry_sequence, key.as_bytes(), &value);
                    //log::info!("Thread {} assertion passed", thread_id);
                }
                Err(err) => {
                    log::error!("Thread {} put error: {}", thread_id, err);
                    eprintln!("Thread {} put error: {}", thread_id, err);
                }
            }
        });
        handles.push(handle);
    }

    // Release the threads to start
    log::info!("Releasing threads to start");
    start_barrier.wait();

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    log::info!("All threads completed");
}

fn assert_entry_matches(entry: WalEntry, entry_sequence: u64, key: &[u8], value: &[u8]) {
    assert_eq!(entry.sequence, entry_sequence);
    assert_eq!(entry.entry_type, PUT_OPERATION);
    assert_eq!(entry.key, key);
    assert_eq!(entry.value, value);
    assert_eq!(entry.key_size, key.len() as u32);
    assert_eq!(entry.value_size, value.len() as u32);
}
