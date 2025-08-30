use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use stone_kvs::wal::crc32c::{crc32c, crc32c_table, crc32c_slice8, crc32c_hw, crc32c_slice32, crc32c_slice16, crc32c_slice16_bt};

fn bench_crc32c(c: &mut Criterion) {
    let mut group = c.benchmark_group("crc32c");

    let sizes = [
        ("1KB", 1024),
        ("10KB", 10 * 1024),
        ("100KB", 100 * 1024),
        ("1MB", 1024 * 1024),
    ];

    let patterns: &[(&str, fn(usize) -> Vec<u8>)] = &[
        ("zeros", |size| vec![0u8; size]),
        ("ones", |size| vec![0xFFu8; size]),
        ("sequential", |size| {
            (0..size).map(|i| (i % 256) as u8).collect()
        }),
        ("random_like", |size| {
            (0..size).map(|i| ((i * 31) % 256) as u8).collect()
        }),
    ];

    sizes
        .iter()
        .flat_map(|(size_name, size)| {
            patterns.iter().map(move |(pattern_name, pattern_fn)| {
                let data = pattern_fn(*size);
                let bench_name = format!("{}_{}", pattern_name, size_name);
                (bench_name, data, *size)
            })
        })
        .for_each(|(bench_name, data, size)| {
            group.throughput(Throughput::Bytes(size as u64));
            
            // Benchmark bit-by-bit implementation
            group.bench_with_input(
                BenchmarkId::new("bit", &bench_name),
                &data,
                |b, data| {
                    b.iter(|| crc32c(data));
                },
            );
            
            // Benchmark table-based implementation
            group.bench_with_input(
                BenchmarkId::new("byte", &bench_name),
                &data,
                |b, data| {
                    b.iter(|| crc32c_table(data));
                },
            );

            // Benchmark slice8 implementation
            group.bench_with_input(
                BenchmarkId::new("8_bytes", &bench_name),
                &data,
                |b, data| {
                    b.iter(|| crc32c_slice8(data));
                },
            );
            
            // Benchmark slice16 implementation
            group.bench_with_input(
                BenchmarkId::new("16_bytes", &bench_name),
                &data,
                |b, data| {
                    b.iter(|| crc32c_slice16(data));
                },
            );

            // Benchmark slice32 implementation
            group.bench_with_input(
                BenchmarkId::new("16_bytes_bt", &bench_name),
                &data,
                |b, data| {
                    b.iter(|| crc32c_slice16_bt(data));
                },
            );
            
            // Benchmark slice32 implementation
            group.bench_with_input(
                BenchmarkId::new("32_byte", &bench_name),
                &data,
                |b, data| {
                    b.iter(|| crc32c_slice32(data));
                },
            );
            
            // Benchmark hardware implementation (only on supported architectures)
            #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
            group.bench_with_input(
                BenchmarkId::new("hardware", &bench_name),
                &data,
                |b, data| {
                    b.iter(|| crc32c_hw(data));
                },
            );
        });

    group.finish();
}

criterion_group!(benches, bench_crc32c);
criterion_main!(benches);
