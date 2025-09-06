# RFC: Write Ahead Log (WAL) Implementation for Key-Value Store

**Status**: Draft  
**Author**: Development Team  
**Date**: August 2025  
**Version**: 1.0

## 1. Introduction

### 1.1 Purpose
This RFC specifies the implementation of a Write Ahead Log (WAL) component in Rust for a key-value store database system. The WAL ensures durability and enables crash recovery by logging all write operations before they are applied to the main storage engine.

### 1.2 Goals
- Provide crash-safe durability guarantees for write operations
- Enable efficient recovery after system failures
- Optimize for write latency while maintaining data integrity
- Integrate seamlessly as a submodule within the key-value store crate

### 1.3 Context
The WAL is a critical component for database durability, ensuring that committed operations can be recovered even after unexpected system crashes. This implementation targets a key-value store with no current transaction support, focusing on simplicity and reliability.

## 2. Requirements

### 2.1 Functional Requirements (Must-Have)

#### 2.1.1 Core WAL Operations
- **R-F-001**: Write operations must be logged sequentially with unique sequence numbers
- **R-F-002**: Support PUT and DELETE operations for key-value pairs
- **R-F-003**: Provide crash recovery by replaying logged operations
- **R-F-004**: Validate data integrity using CRC32C checksums

#### 2.1.2 File Management
- **R-F-005**: Implement fixed-size WAL files with automatic rotation
- **R-F-006**: Use incremental file naming (wal-001.log, wal-002.log, etc.)
- **R-F-007**: Maintain separate metadata file for lifecycle management
- **R-F-008**: Support configurable maximum number of active WAL files

#### 2.1.3 Batching and Performance
- **R-F-009**: Implement hybrid batching based on time and batch size thresholds
- **R-F-010**: Provide synchronous write operations with fsync for durability
- **R-F-011**: Support single-writer architecture with queued operations

### 2.2 Non-Functional Requirements

#### 2.2.1 Performance (Must-Have)
- **R-NF-001**: Optimize for write latency minimization
- **R-NF-002**: Handle key/value sizes in bytes to kilobytes range
- **R-NF-003**: Support write-heavy workloads typical for WAL systems

#### 2.2.2 Reliability (Must-Have)
- **R-NF-004**: Zero data loss tolerance - successful writes must be recoverable
- **R-NF-005**: Fail-fast approach for all error conditions
- **R-NF-006**: Stop recovery at first corrupted entry

#### 2.2.3 Platform Support (Must-Have)
- **R-NF-007**: Target Linux-only deployment initially
- **R-NF-008**: Use minimal external dependencies

### 2.3 Nice-to-Have Requirements
- **R-NH-001**: Future support for adaptive file sizing
- **R-NH-002**: Cross-platform compatibility (Windows, macOS)
- **R-NH-003**: Advanced monitoring and metrics collection

## 3. Architecture

### 3.1 System Overview

The WAL implementation consists of several key components working together to provide durable write logging:

```
┌─────────────────┐    ┌──────────────┐    ┌─────────────────┐
│   Key-Value     │───▶│  WAL Module  │───▶│  File System    │
│   Store         │    │              │    │  (WAL Files)    │
└─────────────────┘    └──────────────┘    └─────────────────┘
                              │
                              ▼
                       ┌──────────────┐
                       │  Metadata    │
                       │  Manager     │
                       └──────────────┘
```

### 3.2 Component Architecture

#### 3.2.1 WAL Writer
- **Responsibility**: Handle write operations and batching
- **Implementation**: Single-threaded writer with operation queue
- **Batching**: Hybrid time/size-based batch flushing

#### 3.2.2 File Manager
- **Responsibility**: Manage WAL file lifecycle and rotation
- **Implementation**: On-demand file allocation with atomic switching
- **Strategy**: Fixed-size files with incremental naming

#### 3.2.3 Metadata Manager
- **Responsibility**: Track active/inactive files and system state
- **Implementation**: Separate metadata file with atomic updates
- **Update Strategy**: Write-then-rename for crash safety

#### 3.2.4 Recovery Engine
- **Responsibility**: Replay WAL entries during system startup
- **Implementation**: Sequential file reading with entry validation
- **Error Handling**: Stop at first corruption

### 3.3 Module Structure

```
src/
├── wal/
│   ├── mod.rs              # Public API and module exports
│   ├── writer.rs           # WAL writer implementation
│   ├── reader.rs           # Recovery and read operations
│   ├── file_manager.rs     # File lifecycle management
│   ├── metadata.rs         # Metadata file operations
│   ├── entry.rs            # WAL entry format and serialization
│   └── config.rs           # Configuration structures
```

## 4. Data Handling

### 4.1 WAL Entry Format

Each WAL entry follows this binary format:

```rust
struct WALEntry {
    sequence_number: u64,      // 8 bytes - Unique identifier
    checksum: u32,             // 4 bytes - CRC32C of remaining fields
    version: u16,              // 2 bytes - Entry format version
    operation_type: u8,        // 1 byte  - PUT(1) or DELETE(2)
    reserved: u8,              // 1 byte  - Future use
    key_size: u32,             // 4 bytes - Length of key
    value_size: u32,           // 4 bytes - Length of value
    key: Vec<u8>,              // Variable - Key data
    value: Vec<u8>,            // Variable - Value data (empty for DELETE)
}
```

**Total Entry Size**: 24 bytes + key_size + value_size

### 4.2 File Format

#### 4.2.1 WAL File Header
```rust
struct WALFileHeader {
    magic: [u8; 8],           // "WALFILE\0"
    version: u16,             // File format version
    reserved: [u8; 6],        // Future use
}
```

#### 4.2.2 File Layout
```
┌─────────────────┐
│   File Header   │  16 bytes
├─────────────────┤
│   WAL Entry 1   │  Variable size
├─────────────────┤
│   WAL Entry 2   │  Variable size
├─────────────────┤
│      ...        │
└─────────────────┘
```

### 4.3 Metadata File Format

```rust
struct MetadataFile {
    active_files: Vec<u64>,    // List of active WAL file numbers
    next_sequence: u64,        // Next sequence number to assign
    last_checkpoint: u64,      // Last checkpointed sequence number
    max_active_files: u32,     // Maximum allowed active files
}
```

### 4.4 Data Flow

1. **Write Path**:
   ```
   Client Write → Batch Buffer → WAL Entry Creation → File Write → fsync → Response
   ```

2. **Recovery Path**:
   ```
   Startup → Read Metadata → Read Active Files → Validate Entries → Apply Operations
   ```

## 5. API Design

### 5.1 Public Interface

```rust
pub struct WAL {
    // Internal implementation details
}

impl WAL {
    /// Initialize WAL with configuration
    pub fn new(config: WALConfig) -> Result<Self, WALError>;
    
    /// Write a single entry to the WAL
    pub fn write_entry(&mut self, op: Operation) -> Result<u64, WALError>;
    
    /// Write multiple entries as a batch
    pub fn write_batch(&mut self, ops: Vec<Operation>) -> Result<Vec<u64>, WALError>;
    
    /// Perform recovery and return recovered operations
    pub fn recover(&self) -> Result<Vec<Operation>, WALError>;
    
    /// Force flush of any pending batched operations
    pub fn flush(&mut self) -> Result<(), WALError>;
    
    /// Clean shutdown with proper file closure
    pub fn close(self) -> Result<(), WALError>;
}

pub enum Operation {
    Put { key: Vec<u8>, value: Vec<u8> },
    Delete { key: Vec<u8> },
}

pub struct WALConfig {
    pub wal_dir: PathBuf,
    pub max_file_size: u64,
    pub max_active_files: u32,
    pub batch_size: usize,
    pub batch_timeout_ms: u64,
}
```

### 5.2 Error Types

```rust
#[derive(Debug, Clone)]
pub enum WALError {
    IoError(String),
    CorruptedEntry { file: String, position: u64 },
    InvalidVersion { expected: u16, found: u16 },
    MaxFilesExceeded { limit: u32 },
    ConfigError(String),
}
```

## 6. Error Handling

### 6.1 Error Handling Strategy

The WAL implementation follows a fail-fast approach for all error conditions:

#### 6.1.1 Write Operation Errors
- **Disk Full**: Return `WALError::IoError`, caller must handle
- **File System Errors**: Propagate immediately to caller
- **Max Files Exceeded**: Return `WALError::MaxFilesExceeded`, stop accepting writes

#### 6.1.2 Recovery Errors
- **Corrupted Entry**: Stop recovery at corruption point, return partial results
- **Missing Files**: Return error if expected active files are missing
- **Version Mismatch**: Reject recovery if WAL version is unsupported

#### 6.1.3 Metadata Errors
- **Corrupted Metadata**: Fatal error, cannot continue operation
- **Metadata Write Failure**: Retry with exponential backoff (3 attempts max)

### 6.2 Error Response Codes

| Error Code | Description | Recovery Action |
|------------|-------------|-----------------|
| WAL_001 | Disk space exhausted | Check disk space, clean old files |
| WAL_002 | Corrupted WAL entry | Manual inspection required |
| WAL_003 | Version mismatch | Update WAL version or downgrade |
| WAL_004 | Max files exceeded | Increase limit or trigger cleanup |
| WAL_005 | Metadata corruption | Restore from backup if available |

## 7. Performance Considerations

### 7.1 Write Performance Optimizations

#### 7.1.1 Batching Strategy
- **Hybrid Trigger**: Combine time-based (configurable ms) and size-based (configurable entries) batching
- **Default Values**: 100 entries or 10ms timeout (configurable)
- **Memory Usage**: Pre-allocate batch buffers to reduce allocation overhead

#### 7.1.2 I/O Optimizations
- **Sequential Writes**: Always append to current file for optimal disk performance
- **fsync Strategy**: Synchronous flush after each batch for durability
- **File Allocation**: On-demand allocation to minimize resource usage

### 7.2 Recovery Performance

#### 7.2.1 Sequential Reading
- **Buffer Size**: Use 64KB read buffer for efficient disk I/O
- **Entry Validation**: Validate checksums during read to fail fast on corruption
- **Memory Management**: Stream processing to handle large WAL files

### 7.3 Performance Targets

- **Write Latency**: < 1ms for single entry (excluding fsync time)
- **Batch Throughput**: > 10,000 entries/second
- **Recovery Speed**: > 50,000 entries/second during sequential read

## 8. Security Measures

### 8.1 File System Security

The WAL implementation relies on filesystem-level security mechanisms:

#### 8.1.1 File Permissions
- **WAL Files**: Standard filesystem permissions (owner read/write)
- **Directory Access**: Restrict WAL directory access through filesystem ACLs
- **No Built-in Encryption**: Rely on filesystem-level encryption if required

#### 8.1.2 Access Control
- **Process Isolation**: WAL files accessible only to database process user
- **Network Security**: No network access required for WAL operations
- **Audit Trail**: Rely on filesystem audit logs for access monitoring

### 8.2 Data Integrity

- **Checksums**: CRC32C validation for every WAL entry
- **Atomic Operations**: File operations designed to be crash-safe
- **Version Control**: Entry and file format versioning for compatibility

## 9. Testing Plan

### 9.1 Unit Testing

#### 9.1.1 Component Tests
- **Entry Serialization**: Test WAL entry encoding/decoding with various data sizes
- **File Operations**: Test file creation, writing, rotation, and cleanup
- **Metadata Management**: Test metadata file operations and atomic updates
- **Checksum Validation**: Test CRC32C calculation and validation

#### 9.1.2 Error Condition Tests
- **Corruption Simulation**: Inject corrupted data and verify error handling
- **Disk Space Tests**: Simulate disk full conditions
- **Partial Write Tests**: Test handling of incomplete write operations

### 9.2 Integration Testing

#### 9.2.1 Crash Recovery Tests
- **Power Failure Simulation**: Kill process during writes, verify recovery
- **Partial Write Recovery**: Test recovery with incomplete final entries
- **Multi-File Recovery**: Test recovery across multiple WAL files

#### 9.2.2 Performance Tests
- **Latency Measurement**: Measure write operation latencies under various loads
- **Throughput Testing**: Test sustained write performance
- **Recovery Speed**: Measure recovery time for various WAL sizes

### 9.3 Property-Based Testing

#### 9.3.1 WAL Invariants
- **Sequence Ordering**: Verify sequence numbers are monotonically increasing
- **Recovery Completeness**: Ensure all successfully written entries are recoverable
- **Batch Atomicity**: Verify batch operations are all-or-nothing

### 9.4 Benchmark Testing

#### 9.4.1 Comparative Analysis
- **WAL Implementations**: Compare against RocksDB WAL, PostgreSQL WAL
- **Metrics**: Latency, throughput, recovery time, disk usage
- **Test Scenarios**: Various key/value sizes, batch sizes, and write patterns

### 9.5 Test Automation

```rust
// Example test structure
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_single_entry_write_recovery() {
        // Test basic write and recovery cycle
    }
    
    #[test] 
    fn test_batch_write_atomicity() {
        // Test batch write behavior
    }
    
    #[test]
    fn test_corruption_handling() {
        // Test response to corrupted entries
    }
}
```

## 10. Implementation Timeline

### 10.1 Development Phases

#### Phase 1: Core Implementation (4 weeks)
- **Week 1**: Entry format and serialization
- **Week 2**: File management and I/O operations
- **Week 3**: Metadata management and file rotation
- **Week 4**: Recovery engine implementation

#### Phase 2: Integration and Testing (3 weeks)
- **Week 5**: Key-value store integration
- **Week 6**: Comprehensive testing suite
- **Week 7**: Performance testing and optimization

#### Phase 3: Validation (2 weeks)
- **Week 8**: Benchmark against other WAL implementations
- **Week 9**: Documentation and final validation

### 10.2 Milestones

1. **M1**: Basic WAL entry write/read functionality
2. **M2**: File rotation and metadata management
3. **M3**: Complete recovery implementation
4. **M4**: Integration with key-value store
5. **M5**: Performance benchmarks complete

## 11. Open Questions and Future Considerations

### 11.1 Open Questions

#### 11.1.1 Configuration Format Decision
- **Question**: Choose between TOML, JSON, or YAML for configuration
- **Impact**: Developer experience and parsing dependencies
- **Decision Timeline**: Before Phase 1 completion

#### 11.1.2 Batch Threshold Optimization
- **Question**: Optimal default values for batch size and timeout
- **Impact**: Write latency vs. throughput balance
- **Resolution**: Determine through performance testing in Phase 2

### 11.2 Future Enhancements

#### 11.2.1 Adaptive File Sizing (Low Priority)
- **Description**: Implement dynamic file sizing based on usage patterns
- **Benefits**: Better disk utilization, optimized I/O patterns
- **Complexity**: Significant increase in file management logic
- **Timeline**: Post-MVP consideration

#### 11.2.2 Cross-Platform Support (Medium Priority)
- **Description**: Extend support to Windows and macOS
- **Benefits**: Broader deployment options
- **Requirements**: Abstract filesystem operations, handle platform differences
- **Timeline**: Future major version

#### 11.2.3 Advanced Monitoring (Low Priority)
- **Description**: Built-in metrics and observability features
- **Benefits**: Better operational visibility
- **Implementation**: Performance counters, structured logging
- **Timeline**: Based on operational requirements

#### 11.2.4 Compression Support (Low Priority)
- **Description**: Optional compression for WAL entries
- **Benefits**: Reduced disk usage for large values
- **Trade-offs**: CPU overhead vs. storage savings
- **Timeline**: Future enhancement based on usage patterns

### 11.3 Risk Mitigation

#### 11.3.1 Performance Risk
- **Risk**: Write latency higher than expected
- **Mitigation**: Early performance testing, profiling, and optimization
- **Contingency**: Implement async I/O if synchronous performance insufficient

#### 11.3.2 Reliability Risk
- **Risk**: Edge cases in crash recovery not properly handled
- **Mitigation**: Extensive crash testing, formal verification of critical paths
- **Contingency**: Implement additional safety checks and validation

---

**End of Specification**

This RFC provides a comprehensive foundation for implementing the WAL component. All major architectural decisions have been documented, and the specification includes sufficient detail for immediate development start.