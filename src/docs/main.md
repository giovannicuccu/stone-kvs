# Benchmark
- The benchmarks are executed with the script `bin/runperf`
- To run the benchmarks, you need to install the following packages:
    - cargo-criterion in cargo
    - cpupower 
- The `bin/runperf` script asks for sudo permissions to set the system to a good state
- An example of how to run the benchmarks is the following (executed from the project root):
    ```bash
    ./bin/runperf ./target/release/deps/crc32c_bench-972d4187f49b455c --bench
    ```
There is a script to run the benchmarks `bin/bench` that automates the process. Simply call:
```bash
./bin/bench <bench-name>
```
for example
```bash
./bin/bench crc32c_bench
```
and you're done.

## CRC32C
- I spent a lot of time trying to understand the CRC32C algorithm and how it works; see some comments in the code
- I saved the most useful document/explanation I found in `src/docs/crc_v3.txt` downloaded from: https://zlib.net/crc_v3.txt
- I saved the file because I want to have all the information in a single place
- The `generate_crc32c_table` function cannot be rewritten in a more idiomatic Rust way because it would lose the `const fn` feature which is mandatory for performance

## Error Management
These are the two referencing for modelling Rust error in this project
- https://www.shuttle.dev/blog/2022/06/30/error-handling
- https://sabrinajewson.org/blog/errors

## WAL
I tried the channel implementation that peaks to 80MB/s doing nothing
I tried the sync implementation that peaks to 45MB7s doing nothing
The rocksdb implementation peaks at 220MB/s writing to disk.... it's a huge difference

I tuned the channel_wal_implmentation using cargo bench (using NoOpStorage) and the results were astonishing
I got a significant performance increase.
I decided to change the mpsc implementation and I switched to crossbeam and I got enormous gains on small key/value pairs and a performance penalty
when the data goes over 16kb. I need to dig further into this topic.
Now 
WALRUS_FSYNC=async WALRUS_DURATION=5s cargo test stonekvs_multithreaded_benchmark -- --no-capture
reports 80MB/sec but it writes to disk (without fsync)

Tokio has some channel structs but they are async based, I skip them for now


