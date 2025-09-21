use std::path::PathBuf;
use std::fs;
use std::io::{Seek, Write};
use tempfile::TempDir;
use stone_kvs::wal::wal::{Wal, WalConfig};

#[test]
fn wal_opens_with_valid_directory_path() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let result = Wal::open(config);

    assert!(result.is_ok());
}

#[test]
fn wal_initializes_sequence_number_to_zero() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let wal = Wal::open(config).unwrap();

    assert_eq!(wal.sequence(), 0);
}

#[test]
fn write_entry_increments_sequence_number() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let mut wal = Wal::open(config).unwrap();
    let initial_sequence = wal.sequence();

    let returned_sequence = wal.write_entry(b"key1", b"value1").unwrap();

    assert_eq!(returned_sequence, initial_sequence + 1);
    assert_eq!(wal.sequence(), initial_sequence + 1);
}

#[test]
fn wal_open_fails_with_non_existent_directory() {
    let non_existent_path = PathBuf::from("/path/that/does/not/exist");
    let config = WalConfig::new(non_existent_path);

    let result = Wal::open(config);

    assert!(result.is_err());
}

#[test]
fn wal_creates_wal_subdirectory_on_open() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path.clone());

    let result = Wal::open(config);

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
    let result = Wal::open(config);

    assert!(result.is_err());
}

#[test]
fn entries_iterator_returns_empty_for_new_wal() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let wal = Wal::open(config).unwrap();
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

    let mut wal = Wal::open(config).unwrap();

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

    let wal = Wal::open(config).unwrap();

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

    let wal = Wal::open(config).unwrap();

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

    let wal = Wal::open(config).unwrap();

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

    let mut wal = Wal::open(config).unwrap();

    wal.write_entry(b"test_key", b"test_value").unwrap();

    let mut file = fs::OpenOptions::new().write(true).open(wal.log_path()).unwrap();

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

    let mut wal = Wal::open(config).unwrap();

    wal.write_entry(b"test_key", b"test_value").unwrap();

    let mut file = fs::OpenOptions::new().write(true).open(wal.log_path()).unwrap();

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

    let mut wal = Wal::open(config).unwrap();

    wal.write_entry(b"test_key", b"test_value").unwrap();
    wal.write_entry(b"test_key2", b"test_value2").unwrap();

    let mut file = fs::OpenOptions::new().write(true).open(wal.log_path()).unwrap();

    file.seek(std::io::SeekFrom::Start(38)).unwrap(); // Skip file header + entry header
    file.write_all(b"CORRUPTED_DATA").unwrap();
    file.flush().unwrap();

    let mut iter = wal.entries().unwrap();
    let _ =iter.next().unwrap();
    assert!(iter.next().is_none());
}

#[test]
fn wal_entry_iterator_fails_with_corrupted_crc32c() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path.clone());

    let mut wal = Wal::open(config).unwrap();
    wal.write_entry(b"test_key", b"test_value").unwrap();

    let mut file = fs::OpenOptions::new().write(true).open(wal.log_path()).unwrap();
    file.seek(std::io::SeekFrom::Start(21)).unwrap(); // Skip 21-byte header
    file.write_all(&[0xFF, 0xFF, 0xFF, 0xFF]).unwrap(); // Corrupt CRC32C

    let mut iter = wal.entries().unwrap();
    let entry_res = iter.next().unwrap();

    assert!(entry_res.is_err());
}