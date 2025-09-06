# Project Plan

## General

- [x] find and save the script for making the benchmarks more deterministic
- [x] write the documentation for the script
- [x] test the script and provide some examples
- [x] create a script that runs the benchmark by name (i.e. it builds the binary and runs the benchmark)
- [x] the script is built for Intel CPUs, add support for other CPUs, I have an AMD one so try to adjust it
- [x] update the script and system config in order to run the script without inserting the system password

## CRC32C Implementation

- [x] Write simplest failing test for CRC32C calculation
- [x] Implement minimum code to make CRC32C test pass
- [x] Add test for empty input
- [x] Add test for known CRC32C values
- [x] Write a byte to byte implementation
- [x] Write a 8 byte implementation
- [x] Write an hardware based implementation 
- [x] Write a 32 byte implementation
- [x] Write a 16 byte implementation

## WAL Implementation
- [x] design the wal struct implementation
- [x] check if the master dir exists and fails if it doesn't
- [ ] check if the wal dir exists and create it if it's missing
- [ ] open operation feature 1 : read the wal dir when no files exist -> it sets the sequence number to 1
- [ ] fail if the file system does not allow directory creation
- [ ] add the ability to write an entry -> it returns a sequence number that should be higher than the initial one
- [ ] add the ability to read entries
- [ ] verify entry corruption
- [ ] verify that after an entry is corrupted the next entries cannot be read
- [ ] performance improvements (to be detailed)
- [ ] metadata and multiple wal log management
