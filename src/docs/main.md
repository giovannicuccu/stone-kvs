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