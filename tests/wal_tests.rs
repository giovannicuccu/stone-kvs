use rand::Rng;
use std::fs::OpenOptions;
use std::io::{Seek, Write};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Barrier, Once};
use std::time::{Duration, Instant};
use std::{fs, thread};
use stone_kvs::wal::{ChannelWal, SyncWal, Wal, WalConfig, WalEntry};
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
fn wal_opens_with_valid_directory_path() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let result = SyncWal::open(config);

    assert!(result.is_ok());
}

#[test]
fn wal_initializes_sequence_number_to_zero() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let wal = SyncWal::open(config).unwrap();

    assert_eq!(wal.sequence(), 0);
}

#[test]
fn write_entry_increments_sequence_number() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let mut wal = SyncWal::open(config).unwrap();
    let initial_sequence = wal.sequence();

    let returned_sequence = wal.write_entry(b"key1", b"value1").unwrap();

    assert_eq!(returned_sequence, initial_sequence + 1);
    assert_eq!(wal.sequence(), initial_sequence + 1);
}

#[test]
fn wal_open_fails_with_non_existent_directory() {
    let non_existent_path = PathBuf::from("/path/that/does/not/exist");
    let config = WalConfig::new(non_existent_path);

    let result = SyncWal::open(config);

    assert!(result.is_err());
}

#[test]
fn wal_creates_wal_subdirectory_on_open() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path.clone());

    let result = SyncWal::open(config);

    assert!(result.is_ok());
    let wal_subdir = wal_path.join("wal");
    assert!(wal_subdir.exists());
    assert!(wal_subdir.is_dir());
}

#[test]
fn wal_open_fails_when_directory_is_read_only() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();

    // Make directory read-only
    let mut permissions = fs::metadata(&wal_path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&wal_path, permissions).unwrap();

    let config = WalConfig::new(wal_path);
    let result = SyncWal::open(config);

    assert!(result.is_err());
}

#[test]
fn entries_iterator_returns_empty_for_new_wal() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let wal = SyncWal::open(config).unwrap();
    let iter = wal.entries().unwrap();
    let entries: Result<Vec<_>, _> = iter.collect();

    assert!(entries.is_ok());
    assert_eq!(entries.unwrap().len(), 0);
}

#[test]
fn write_entry_can_be_read_back() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let mut wal = SyncWal::open(config).unwrap();

    let returned_sequence = wal.write_entry(b"test_key", b"test_value").unwrap();

    let mut iter = wal.entries().unwrap();
    let entry = iter.next().unwrap().unwrap();

    assert_eq!(entry.sequence, returned_sequence);
    assert_eq!(entry.entry_type, 1);
    assert_eq!(entry.key, b"test_key");
    assert_eq!(entry.value, b"test_value");
    assert_eq!(entry.key_size, 8);
    assert_eq!(entry.value_size, 10);
}

#[test]
fn wal_entry_iterator_empty_for_non_existent_file() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let wal = SyncWal::open(config).unwrap();

    // Get iterator for non-existent file (no entries written yet)
    let iter = wal.entries().unwrap();
    let entries: Vec<_> = iter.collect();

    assert_eq!(entries.len(), 0);
}

#[test]
fn wal_entry_iterator_fail_for_non_existent_file() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path.clone());

    let wal = SyncWal::open(config).unwrap();

    if let Ok(entries) = fs::read_dir(wal_path) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_dir() {
                    fs::remove_dir_all(&path).unwrap();
                } else {
                    fs::remove_file(&path).unwrap();
                }
            }
        }
    }

    // Get iterator for non-existent file (no entries written yet)
    let result = wal.entries();

    assert!(result.is_err());
}

#[test]
fn wal_entry_iterator_fails_with_invalid_header() {
    use std::io::Write;

    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path.clone());

    let wal = SyncWal::open(config).unwrap();

    // Create a file with invalid header
    let mut file = fs::File::create(wal.log_path()).unwrap();
    file.write_all(b"INVALID_HEADER___").unwrap();

    // Try to create iterator through public API - this should fail at creation time
    let result = wal.entries();

    assert!(result.is_err());
}

#[test]
fn write_entry_cannot_be_read_back_if_corrupted_entry_data() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path.clone());

    let mut wal = SyncWal::open(config).unwrap();

    wal.write_entry(b"test_key", b"test_value").unwrap();

    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(wal.log_path())
        .unwrap();

    file.seek(std::io::SeekFrom::Start(38)).unwrap(); // Skip file header + entry header
    file.write_all(b"CORRUPTED_DATA").unwrap();
    file.flush().unwrap();

    let mut iter = wal.entries().unwrap();
    let entry_res = iter.next().unwrap();
    assert!(entry_res.is_err());
}

#[test]
fn write_entry_cannot_be_read_back_if_corrupted_entry_header() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path.clone());

    let mut wal = SyncWal::open(config).unwrap();

    wal.write_entry(b"test_key", b"test_value").unwrap();

    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(wal.log_path())
        .unwrap();

    file.seek(std::io::SeekFrom::Start(24)).unwrap(); // Skip file header + entry header
    file.write_all(b"CORRUPTED_DATA").unwrap();
    file.flush().unwrap();

    let mut iter = wal.entries().unwrap();
    let entry_res = iter.next().unwrap();
    assert!(entry_res.is_err());
}

#[test]
fn write_entry_cannot_be_read_entry_after_a_corrupted_one_in_entry_data() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path.clone());

    let mut wal = SyncWal::open(config).unwrap();

    wal.write_entry(b"test_key", b"test_value").unwrap();
    wal.write_entry(b"test_key2", b"test_value2").unwrap();

    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(wal.log_path())
        .unwrap();

    file.seek(std::io::SeekFrom::Start(38)).unwrap(); // Skip file header + entry header
    file.write_all(b"CORRUPTED_DATA").unwrap();
    file.flush().unwrap();

    let mut iter = wal.entries().unwrap();
    let _ = iter.next().unwrap();
    assert!(iter.next().is_none());
}

#[test]
fn wal_entry_iterator_fails_with_corrupted_crc32c() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path.clone());

    let mut wal = SyncWal::open(config).unwrap();
    wal.write_entry(b"test_key", b"test_value").unwrap();

    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(wal.log_path())
        .unwrap();
    file.seek(std::io::SeekFrom::Start(21)).unwrap(); // Skip 21-byte header
    file.write_all(&[0xFF, 0xFF, 0xFF, 0xFF]).unwrap(); // Corrupt CRC32C

    let mut iter = wal.entries().unwrap();
    let entry_res = iter.next().unwrap();

    assert!(entry_res.is_err());
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
    let value = vec![1u8; (BLOCK_SIZE / 2) + 200];

    let (entry_sequence, _, value) = wal.write_entry(key.to_vec(), value).unwrap();

    let mut iter = wal.entries().unwrap();
    let _ = iter.next().unwrap().unwrap();
    let _ = iter.next().unwrap().unwrap();
    let entry = iter.next().unwrap().unwrap();

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
