//! Lightweight append-only logging to a file, so a reported issue can be diagnosed
//! from the recorded event/emit sequence instead of a live repro. Every key the tap
//! handles records the character, frontmost app, mode, the decision, the emitted
//! diff, and the engine's raw/rendered state — enough to trace any typing bug.
//!
//! Log file: `~/Library/Logs/GlowKey/glowkey.log`. Appended to across runs, and
//! truncated when it grows past [`MAX_BYTES`] so it never fills the disk. Each line
//! carries a sequence number and seconds-since-start, so ordering and the timing
//! deltas that matter for delivery races are both visible.
//!
//! Note: this records the text you type (that is the point — it is a typing engine).
//! It stays on the local machine; delete the file any time to clear it.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Truncate the log once it passes this size (5 MB) so it is self-bounding.
const MAX_BYTES: u64 = 5_000_000;

fn process_start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

fn next_seq() -> u64 {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    SEQ.fetch_add(1, Ordering::Relaxed)
}

fn log_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let mut path = PathBuf::from(home);
    path.push("Library/Logs/GlowKey/glowkey.log");
    Some(path)
}

/// The shared append handle, opened once. `None` if the path or open failed (logging
/// then silently does nothing — it must never affect typing).
fn handle() -> Option<&'static Mutex<File>> {
    static FILE: OnceLock<Option<Mutex<File>>> = OnceLock::new();
    FILE.get_or_init(|| {
        let path = log_path()?;
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Start fresh if the previous log grew too large.
        if std::fs::metadata(&path)
            .map(|m| m.len() > MAX_BYTES)
            .unwrap_or(false)
        {
            let _ = std::fs::remove_file(&path);
        }
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()
            .map(Mutex::new)
    })
    .as_ref()
}

/// Appends one line to the log (and echoes to stderr when `GLOWKEY_DEBUG` is set).
/// Never panics; a logging failure is swallowed so it cannot disturb input.
pub fn log(message: &str) {
    let line = format!(
        "#{:<5} +{:8.3}s  {}\n",
        next_seq(),
        process_start().elapsed().as_secs_f64(),
        message
    );
    if let Some(mutex) = handle() {
        if let Ok(mut file) = mutex.lock() {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }
    if crate::log::stderr_echo() {
        eprint!("{line}");
    }
}

/// Whether to also echo to stderr (the foreground `dev-run.sh` loop), gated on
/// `GLOWKEY_DEBUG` so a normal launch only writes the file.
fn stderr_echo() -> bool {
    static ECHO: OnceLock<bool> = OnceLock::new();
    *ECHO.get_or_init(|| std::env::var_os("GLOWKEY_DEBUG").is_some())
}
