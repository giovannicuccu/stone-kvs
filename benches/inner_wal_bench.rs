use criterion::{Criterion, criterion_group, criterion_main};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use stone_kvs::wal::NoOpWalConfig;
use stone_kvs::wal::{InnerWal, WalConfig};
use tempfile::TempDir;
fn bench_inner_wal(c: &mut Criterion) {
    let config = NoOpWalConfig {};
    /*
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().to_path_buf();
    let config = WalConfig::new(wal_path);*/
    let mut wal = InnerWal::open(config, Arc::from(AtomicU64::new(0))).unwrap();
    let data = vec![1u8; 1024];
    let key = format!("{}:{}", "topic", 1).as_bytes().to_vec();
    c.bench_function("Inner Wal", |b| {
        b.iter(|| wal.write_entry(key.clone(), data.clone()))
    });
}

criterion_group!(benches, bench_inner_wal);
criterion_main!(benches);
