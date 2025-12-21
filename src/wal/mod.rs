mod channel_wal;
pub mod crc32c;
mod sync_wal;
mod wal_commons;

// Re-export common types
pub use channel_wal::ChannelWal;
pub use channel_wal::InnerWal;
pub use sync_wal::SyncWal;
pub use wal_commons::{WalConfig, WalEntry, WalEntryIterator, WalError, WalErrorKind};

// The Wal trait
pub trait Wal {
    fn write_entry(&self, key: &[u8], value: &[u8]) -> Result<u64, WalError>;
    fn entries(&self) -> Result<WalEntryIterator, WalError>;
    fn sequence(&self) -> u64;
}
