use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use stone_kvs::wal::crc32c::crc32c;

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
        ("sequential", |size| (0..size).map(|i| (i % 256) as u8).collect()),
        ("random_like", |size| (0..size).map(|i| ((i * 31) % 256) as u8).collect()),
    ];
    
    sizes.iter()
        .flat_map(|(size_name, size)| {
            patterns.iter().map(move |(pattern_name, pattern_fn)| {
                let data = pattern_fn(*size);
                let bench_name = format!("{}_{}", pattern_name, size_name);
                (bench_name, data, *size)
            })
        })
        .for_each(|(bench_name, data, size)| {
            group.throughput(Throughput::Bytes(size as u64));
            group.bench_with_input(
                BenchmarkId::new("bit_by_bit", bench_name),
                &data,
                |b, data| {
                    b.iter(|| crc32c(data));
                },
            );
        });
    
    group.finish();
}

criterion_group!(benches, bench_crc32c);
criterion_main!(benches);