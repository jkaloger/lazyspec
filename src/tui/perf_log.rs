use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::OnceLock;
use std::time::Instant;

static LOG_FILE: OnceLock<Option<File>> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();

fn log_file() -> Option<&'static File> {
    LOG_FILE
        .get_or_init(|| {
            if std::env::var("LAZYSPEC_LOG").is_err() {
                return None;
            }
            let path = std::env::var("LAZYSPEC_LOG_PATH")
                .unwrap_or_else(|_| "/tmp/lazyspec-tui.log".to_string());
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .ok()
        })
        .as_ref()
}

fn elapsed_ms() -> f64 {
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_secs_f64() * 1000.0
}

pub fn enabled() -> bool {
    log_file().is_some()
}

pub fn log(msg: &str) {
    if let Some(file) = log_file() {
        let _ = writeln!(&*file, "[{:12.3}ms] {}", elapsed_ms(), msg);
    }
}

pub fn log_duration(label: &str, start: Instant) {
    if enabled() {
        let dur = start.elapsed();
        log(&format!("{}: {:.3}ms", label, dur.as_secs_f64() * 1000.0));
    }
}
