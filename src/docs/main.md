# Benchmark
- the benchmarks are executed with the script `bin/runperf`
- In order to run the benchmarks, I need to install the following packages:
    - cargo-criterion
- the `bin/runperf` script asks for sudo permissions to set the system to a good state
- an exmample of how to run the benchmarks is the following (executed from the project root):
    ```bash
    ./bin/runperf ./target/release/deps/crc32c_bench-972d4187f49b455c --bench
    ```