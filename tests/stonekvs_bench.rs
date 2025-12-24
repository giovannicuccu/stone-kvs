use rand::Rng;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Barrier, mpsc};
use std::time::{Duration, Instant};
use std::{fs, thread};
use stone_kvs::wal::{ChannelWal, InnerWal, WalConfig};

//#[test]
fn stonekvs_multithreaded_bench() {
    let db_path = "stonekvs_benchmark_db";
    let wal_path = format!("{}/wal", db_path);
    cleanup_path(db_path);

    fs::create_dir_all(&wal_path).ok();

    let config = WalConfig::new(PathBuf::from(wal_path.clone()));

    let wal = Arc::new(ChannelWal::open(config).unwrap());
    let num_threads = 10;
    let write_duration = Duration::from_secs(10);

    let start_barrier = Arc::new(Barrier::new(num_threads + 1));
    let write_end_barrier = Arc::new(Barrier::new(num_threads + 1));

    let topics: Vec<String> = (0..num_threads).map(|i| format!("thread_{}", i)).collect();
    let (throughput_tx, throughput_rx) = mpsc::channel::<()>();
    let mut handles = Vec::new();

    for thread_id in 0..num_threads {
        let db_clone = Arc::clone(&wal);
        let start_barrier_clone = Arc::clone(&start_barrier);
        let write_end_barrier_clone = Arc::clone(&write_end_barrier);
        let topic = topics[thread_id].clone();

        let handle = thread::spawn(move || {
            start_barrier_clone.wait();

            let start_time = Instant::now();
            let mut counter = 0u64;

            let mut rng = rand::thread_rng();
            let batch_delay = Duration::from_millis(500);
            let mut batch_number = 0;

            while start_time.elapsed() < write_duration {
                let current_batch_size = match batch_number {
                    0 => 50_000,
                    1 => 100_000,
                    2 => 150_000,
                    3 => 200_000,
                    4 => 250_000,
                    5 => 300_000,
                    6 => 350_000,
                    7 => 400_000,
                    8 => 450_000,
                    _ => 500_000,
                };

                for _ in 0..current_batch_size {
                    if start_time.elapsed() >= write_duration {
                        break;
                    }

                    let size = rng.gen_range(500..=1024);
                    let data = vec![((counter % 256) + 1024) as u8; size];
                    let key = format!("{}:{}", topic, counter);

                    match db_clone.write_entry(key.as_bytes().to_vec(), data) {
                        Ok((_, _, _)) => {}
                        Err(err) => {
                            eprintln!("Thread {} put error: {} (topic {})", thread_id, err, topic);
                        }
                    }

                    counter += 1;
                }

                batch_number += 1;

                if start_time.elapsed() < write_duration {
                    thread::sleep(batch_delay);
                }
            }

            write_end_barrier_clone.wait();
        });

        handles.push(handle);
    }

    start_barrier.wait();
    println!("All threads started! StoneKVS Write phase beginning...");
    let _ = throughput_tx.send(());

    write_end_barrier.wait();

    for handle in handles {
        handle.join().expect("Writer thread panicked");
    }
}

#[test]
fn stonekvs_innerwal_bench() {
    let db_path = "stonekvs_benchmark_db";
    let wal_path = format!("{}/wal", db_path);
    cleanup_path(db_path);

    fs::create_dir_all(&wal_path).ok();

    let config = WalConfig::new(PathBuf::from(wal_path.clone()));
    let write_duration = Duration::from_secs(10);
    let start_time = Instant::now();
    let mut counter = 0u64;

    let mut rng = rand::thread_rng();
    let batch_delay = Duration::from_millis(500);
    let mut batch_number = 0;
    let mut wal = InnerWal::open(config, Arc::from(AtomicU64::new(0)))
        .ok()
        .unwrap();

    while start_time.elapsed() < write_duration {
        let current_batch_size = match batch_number {
            0 => 50_000,
            1 => 100_000,
            2 => 150_000,
            3 => 200_000,
            4 => 250_000,
            5 => 300_000,
            6 => 350_000,
            7 => 400_000,
            8 => 450_000,
            _ => 500_000,
        };

        for _ in 0..current_batch_size {
            if start_time.elapsed() >= write_duration {
                break;
            }

            let size = rng.gen_range(500..=1024);
            let data = vec![((counter % 256) + 1024) as u8; size];
            let key = format!("{}:{}", "topic", counter);

            match wal.write_entry(key.as_bytes().to_vec(), data) {
                Ok((_, _, _)) => {}
                Err(err) => {
                    eprintln!("put error: {}", err);
                }
            }

            counter += 1;
        }

        batch_number += 1;

        if start_time.elapsed() < write_duration {
            thread::sleep(batch_delay);
        }
    }
}

fn cleanup_path(path: &str) {
    let _ = fs::remove_dir_all(path);
    thread::sleep(Duration::from_millis(100));
}
