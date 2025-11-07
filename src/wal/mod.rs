pub mod crc32c;
mod wal_commons;
mod sync_wal;
mod channel_wal;

// Re-export common types
pub use wal_commons::{WalConfig, WalEntry, WalEntryIterator, WalError, WalErrorKind};
pub use sync_wal::SyncWal;
pub use channel_wal::ChannelWal;

// The Wal trait
pub trait Wal {
    fn write_entry(&self, key: &[u8], value: &[u8]) -> Result<u64, WalError>;
    fn entries(&self) -> Result<WalEntryIterator, WalError>;
    fn sequence(&self) -> u64;
}