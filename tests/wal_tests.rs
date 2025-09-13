use std::path::PathBuf;
use std::fs;
use tempfile::TempDir;
use stone_kvs::wal::{Wal, WalConfig, WalEntry};

#[test]
fn wal_opens_with_valid_directory_path() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let result = Wal::open(config);

    assert!(result.is_ok(), "WAL should open successfully with valid directory path");
}

#[test]
fn wal_initializes_sequence_number_to_zero() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let wal = Wal::open(config).expect("WAL should open successfully");

    assert_eq!(wal.sequence(), 0, "WAL sequence number should be initialized to 0");
}

#[test]
fn write_entry_increments_sequence_number() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let mut wal = Wal::open(config).expect("WAL should open successfully");
    let initial_sequence = wal.sequence();

    let returned_sequence = wal.write_entry(b"key1", b"value1").expect("Write entry should succeed");

    assert_eq!(returned_sequence, initial_sequence + 1, "Returned sequence should be incremented by 1");
    assert_eq!(wal.sequence(), initial_sequence + 1, "WAL sequence should be incremented by 1");
}

#[test]
fn wal_open_fails_with_non_existent_directory() {
    let non_existent_path = PathBuf::from("/path/that/does/not/exist");
    let config = WalConfig::new(non_existent_path);
    
    let result = Wal::open(config);
    
    assert!(result.is_err(), "WAL should fail to open with non-existent directory");
}

#[test]
fn wal_creates_wal_subdirectory_on_open() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path.clone());
    
    let result = Wal::open(config);
    
    assert!(result.is_ok(), "WAL should open successfully");
    let wal_subdir = wal_path.join("wal");
    assert!(wal_subdir.exists(), "WAL subdirectory should be created");
    assert!(wal_subdir.is_dir(), "WAL subdirectory should be a directory");
}

#[test]
fn wal_open_fails_when_directory_is_read_only() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let wal_path = temp_dir.path().to_path_buf();
    
    // Make directory read-only
    let mut permissions = fs::metadata(&wal_path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&wal_path, permissions).expect("Failed to set read-only permissions");
    
    let config = WalConfig::new(wal_path);
    let result = Wal::open(config);
    
    assert!(result.is_err(), "WAL should fail to open when directory is read-only");
}

#[test]
fn entries_iterator_returns_empty_for_new_wal() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let wal = Wal::open(config).expect("WAL should open successfully");
    let entries: Result<Vec<_>, _> = wal.entries().collect();

    assert!(entries.is_ok(), "Iterator should work for empty WAL");
    assert_eq!(entries.unwrap().len(), 0, "Empty WAL should have no entries");
}

#[test]
fn write_entry_can_be_read_back() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);

    let mut wal = Wal::open(config).expect("WAL should open successfully");
    
    let returned_sequence = wal.write_entry(b"test_key", b"test_value").expect("Write entry should succeed");
    
    let mut iter = wal.entries();
    let entry = iter.next().expect("Should have one entry").expect("Entry should be valid");
    
    assert_eq!(entry.sequence, returned_sequence, "Entry sequence should match returned sequence");
    assert_eq!(entry.entry_type, 1, "Entry type should be PUT (1)");
    assert_eq!(entry.key, b"test_key", "Key should match written key");
    assert_eq!(entry.value, b"test_value", "Value should match written value");
    assert_eq!(entry.key_size, 8, "Key size should be 8 bytes");
    assert_eq!(entry.value_size, 10, "Value size should be 10 bytes");
    
    // Verify CRC32C checksum using the entry's method
    use stone_kvs::wal::crc32c::crc32c;
    let expected_crc = crc32c(&entry.data_for_crc());
    assert_eq!(entry.crc32c, expected_crc, "CRC32C should match calculated value");
    
    assert!(iter.next().is_none(), "Should have no more entries");
}