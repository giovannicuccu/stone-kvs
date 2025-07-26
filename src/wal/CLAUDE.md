
## Module Goals

- Implement a WAL (Write-Ahead Log) for persistent storage of key value pairs
- The role of the WAL is to persist the key value pairs to disk so that they can be recovered in case of a crash
- The real persistence is handled by the storage layer so when the entries are written in the store there will be no need for the WAL

## WAL Record Format

After evaluating various WAL implementations (RocksDB, LevelDB, SQLite, PostgreSQL), we chose a simplified record-based format optimized for single-threaded sequential writes:

### File Header
```
[Magic(4B) | Version(4B) | Reserved(8B)]
```

### Record Format
```
[Type(1B) | Sequence(8B) | CRC32C(4B) | Key_Size(4B) | Value_Size(4B) | Key | Value]
```

### Design Decisions

1. **Sequence Numbers**: Each record includes a monotonically increasing 8-byte sequence number to establish processing order and track which WAL records have been written to the database file. This helps with recovery and ensures data consistency.

2. **No Total Length Field**: Redundant since `Total_Length = 21 + Key_Size + Value_Size`. This saves 4 bytes per record.

3. **Integrity Checking**: After evaluating various algorithms (CRC32, CRC32C, xxHash3), we chose **CRC32C with SSE4.2 hardware acceleration** for optimal performance on modern Intel/AMD CPUs:
   - 3-5x faster than software CRC32, can achieve 3.3+ GB/s
   - Uses SSE4.2 `crc32` instruction available on all modern Intel/AMD CPUs (2010+)
   - Minimal implementation (~50 lines) with zero external dependencies
   - Compact 4-byte checksum (vs 8-byte for XXH3)
   - Proven in production (ext4, Btrfs, PostgreSQL)
   - Each record is independently validated
   - EstimatedPerformance Comparison (Benchmarks on modern Intel/AMD CPUs)
     - **CRC32C (SSE4.2)**: ~3.3 GB/s, 3 cycles latency, 1 cycle throughput
     - **XXH3**: ~4 GB/s but requires larger implementation
     - **Software CRC32**: ~1 GB/s
     - **CRC32C vs XXH3**: 3-5x faster than software CRC32, competitive with XXH3 while being simpler to implement

4. **Version Field**: Enables non-backward-compatible changes and handling different format versions.

5. **Type Field**: Supports different operations (PUT, DELETE) for KV store semantics.

### Record Types
- `0x01`: PUT operation
- `0x02`: DELETE operation

### Recovery Strategy
- Skip corrupted records (CRC32C mismatch) and continue
- Use file position for ordering
- Replay all valid records in sequence



