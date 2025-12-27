use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use stone_kvs::wal::{ChannelWal, NoOpWalConfig};

fn bench_channel_wal(c: &mut Criterion) {
    let mut group = c.benchmark_group("Channel Wal");
    let sizes = [
        ("1KB", 1024),
        ("4KB", 4 * 1024),
        ("16KB", 16 * 1024),
        ("32KN", 32 * 1024),
    ];
    sizes
        .iter()
        .map(|(size_name, size)| {
            let config = NoOpWalConfig {};
            let wal = ChannelWal::open(config).unwrap();
            let data = vec![1u8; 1024];
            let key = format!("{}:{}", "topic", 1).as_bytes().to_vec();
            let bench_name = format!("{}", size_name);
            (bench_name, key, data, wal)
        })
        .for_each(|(bench_name, key, data, wal)| {
            // Benchmark bit-by-bit implementation
            group.bench_with_input(
                BenchmarkId::new("", &bench_name),
                &(key.clone(), data.clone()),
                |b, _| {
                    b.iter(|| wal.write_entry(key.clone(), data.clone()));
                },
            );
        });
    group.finish();
}

criterion_group!(benches, bench_channel_wal);
criterion_main!(benches);
