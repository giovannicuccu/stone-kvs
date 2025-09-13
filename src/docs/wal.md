
## Module Goals

- Implement a WAL (Write-Ahead Log) for persistent storage of key value pairs
- The role of the WAL is to persist the key value pairs to disk so that they can be recovered in case of a crash
- The real persistence is handled by the storage layer so when the entries are written in the store there will be no need for the WAL

# Write Ahead Log (WAL) Implementation Specification

## Overview

This document outlines the detailed specification for a Write Ahead Log (WAL) implementation in Rust, designed as a component for a key-value store database system.

## System Requirements

### Purpose and Scope
- **Primary Use Case**: Component for a key-value store database system
- **Transaction Support**: Not planned in initial implementation
- **Target Platform**: Linux only
- **Concurrency Model**: Single writer design with batch write operations

### Data Characteristics
- **Key/Value Size Range**: Bytes to kilobytes
- **Access Pattern**: Write-heavy workload (typical for WAL)
- **Durability Requirements**: Synchronous disk flushes (fsync) after every write operation
- **Data Loss Tolerance**: Zero - if WAL returns OK, data must be recoverable

## WAL Entry Format

### Entry Structure
Each WAL entry contains the following fields in order:
- **Sequence Number**: Unique identifier for entry ordering
- **Checksum**: CRC32C for integrity verification
- **Version**: Entry format version
- **Operation Type**: Type of operation (PUT/DELETE)
- **Key Size**: Length of the key in bytes
- **Value Size**: Length of the value in bytes
- **Key**: The actual key data
- **Value**: The actual value data

## File Format and Organization

### File Structure
- **File Organization**: Sequential write of entries to binary files
- **File Header**: Contains magic number, version, and reserved field
- **File Naming**: Incrementing numbers (wal-001.log, wal-002.log, etc.)
- **File Size Strategy**: Fixed size approach (initially), with future consideration for adaptive sizing

### File Lifecycle Management
- **Metadata Storage**: Separate metadata file tracking active/inactive WAL segments
- **Metadata Atomicity**: Write-then-rename approach (write to temporary file, fsync, then rename)
- **File Rotation**: On-demand allocation with atomic file switch mechanism
- **Cleanup Strategy**: Maintain maximum number of active files, stop program if threshold exceeded

## Write Operations

### Batching Strategy
- **Batch Trigger**: Hybrid approach using both time-based and batch size thresholds
- **Latency Optimization**: Minimize write operation latency
- **Concurrency**: Single writer thread handling all write operations

### Error Handling
- **Write Failures**: Immediately return errors to callers
- **Resource Exhaustion**: Rely on operating system error reporting
- **Corruption Handling**: Stop recovery at first corrupted entry encountered

## Recovery Mechanism

### Recovery Process
- **Recovery Strategy**: Sequential read of all active WAL files from oldest to newest
- **Entry Validation**: Validate individual entry checksums during recovery
- **Corruption Response**: Stop recovery process when corrupted entry is encountered
- **Startup Behavior**: Use metadata file to determine next sequence number and validate integrity

### Version Compatibility
- **Version Support**: Recovery allows only specific WAL version
- **Version Mismatch**: Reject and stop processing unsupported versions

## Configuration and Deployment

### Configuration Management
- **Configuration Format**: Configuration file (format TBD - TOML, JSON, or YAML)
- **Settings Include**: File paths, batch sizes, time thresholds, maximum active files

### Integration
- **Module Structure**: WAL implemented as submodule within key-value store crate
- **API Design**: Synchronous interface with methods like `write_entry()` and `recover()`
- **Build Strategy**: Integrated directly into key-value store crate

## Implementation Details

### Dependencies
- **External Crates**: Minimal usage - implement core functionalities (CRC32C) in-house
- **Standard Library**: Primary reliance on Rust standard library for file I/O

### Memory Management
- **Buffer Strategy**: Simple approach using dynamic allocation with Rust's Vec
- **Memory Pools**: Not implemented initially

### Coding Standards
- **Naming Conventions**: Standard Rust conventions (snake_case for functions/variables, PascalCase for types)
- **Public API**: Follow main crate coding rules
- **Internal Implementation**: May deviate from general rules only when necessary

## Security and Monitoring

### Security
- **File Access**: Rely on filesystem-level security mechanisms
- **Permissions**: Standard filesystem permissions
- **Encryption**: Not implemented

### Observability
- **Monitoring**: Minimal implementation without built-in observability features
- **Logging**: Basic logging for debugging purposes
- **Metrics**: Not implemented initially

### Documentation
- **Documentation Strategy**: Design documents and usage examples in git repository
- **API Documentation**: Inline code comments for maintainability

## Testing Strategy

### Test Coverage
- **Unit Tests**: Individual components (entry serialization, file operations)
- **Integration Tests**: Crash scenarios and recovery simulation
- **Property-Based Testing**: WAL invariants under various failure conditions
- **Performance Testing**: Compare against other WAL implementations

## Development Timeline

### Implementation Approach
- **Development Strategy**: Complete WAL implementation before key-value store integration
- **Feature Priority**: All core features (batching, file rotation) implemented before integration
- **Phased Delivery**: Single comprehensive implementation rather than iterative approach

## Edge Cases and Error Scenarios

### Error Handling Strategy
- **Disk Full**: Stop program
- **Corrupted Metadata**: Stop program
- **Partial File Deletions**: Stop program
- **General Philosophy**: Fail-fast approach for all edge cases

## Future Considerations

### Potential Enhancements
- **Adaptive File Sizing**: Evaluate complexity vs. benefits for future implementation
- **Cross-Platform Support**: Windows compatibility in future versions
- **Advanced Monitoring**: Performance counters and detailed metrics
- **Encryption**: File-level encryption for sensitive deployments

---

*This specification represents the initial design based on requirements analysis and serves as the foundation for implementation.*

## WAL Record Format

After evaluating various WAL implementations (RocksDB, LevelDB, SQLite, PostgreSQL), we chose a simplified record-based format optimized for single-threaded sequential writes:

### File Header
```
[Magic(4B) | Version(4B) | Reserved(8B)]
```

### Record Format
```
[ CRC32C(4B) | Sequence(8B) | Type(1B) | Key_Size(4B) | Value_Size(4B) | Key | Value]
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


### Implementation
The implementation will start simple and will evolve over time. The starting point is a WAL struct

#### Sequence
The wal holds a sequence number, it's incremented whenever an entry is added. It the wal is created from scratch the sequence number starts from 0.
The sequence number has type u64.

#### Write operation
the wal offers `write_entry` method for adding a single entry, the method params are key and value and they are u8 slices.
the method increments the sequence number encodes the operation with the value defined in this file and serializes the content, 
following the `Record Format` defined in this file.
the content must be written in the wal file. the wal file is named 'wal.log' and it resides in the wal dir. 
If the file does not exists it must be created on wal creation. the file must be initialized with the `File Header` defined in thi document
The content of the file must be persisted on file system, i.e. if the powers goes down just after the entry is written the entry should be readable from disk.
the `write_entry` returns the corresponding sequence number

### Read operation
The read operation is exposed via an iterator. the iterator expose a different wal entry from the write operation since it must expose all the data written. 
The element returned by the iterator must follow the `Record Format` defined in this document

