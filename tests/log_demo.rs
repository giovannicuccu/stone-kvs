use std::fs::OpenOptions;
use std::sync::Once;
use std::thread;
use std::time::Duration;

static INIT: Once = Once::new();

fn init_test_logger() {
    INIT.call_once(|| {
        let log_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open("test_thread_macro.log")
            .expect("Failed to open log file");

        env_logger::Builder::new()
            .target(env_logger::Target::Pipe(Box::new(log_file)))
            .filter_level(log::LevelFilter::Debug)
            .init();
    });
}

// Your struct that spawns a thread
struct Worker {
    name: String,
}

impl Worker {
    fn new(name: String) -> Self {
        Self { name }
    }

    fn start(&self) {
        let name = self.name.clone();

        thread::spawn(move || {
            // Now you can use log! macros from the thread - they work automatically!
            log::info!("Thread '{}' started", name);

            for i in 0..3 {
                thread::sleep(Duration::from_millis(100));
                log::debug!("Thread '{}' working... step {}", name, i);
            }

            log::info!("Thread '{}' finished", name);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_with_log_macros() {
        init_test_logger(); // Initialize once

        log::info!("[TEST] Starting test_worker_with_log_macros");

        let worker = Worker::new("worker-1".to_string());
        worker.start();

        thread::sleep(Duration::from_millis(500));

        log::info!("[TEST] Test completed");
    }

    #[test]
    fn test_multiple_workers_with_macros() {
        init_test_logger();

        log::info!("[TEST] Starting test with multiple workers");

        let worker1 = Worker::new("worker-1".to_string());
        let worker2 = Worker::new("worker-2".to_string());
        let worker3 = Worker::new("worker-3".to_string());

        worker1.start();
        worker2.start();
        worker3.start();

        thread::sleep(Duration::from_millis(500));

        log::info!("[TEST] All workers completed");
    }
}
