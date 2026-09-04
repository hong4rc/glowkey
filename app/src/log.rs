//! Lightweight append-only logging to a file, so a reported issue can be diagnosed
//! from the recorded event/emit sequence instead of a live repro. Every key the tap
//! handles records the character, frontmost app, mode, the decision, the emitted
//! diff, and the engine's raw/rendered state — enough to trace any typing bug.
//!
//! Log file: `~/Library/Logs/GlowKey/glowkey.log`. Appended to across runs, and
//! **rotated** once it passes [`MAX_BYTES`] so it never fills the disk. Each line
//! carries a sequence number and seconds-since-start, so ordering and the timing
//! deltas that matter for delivery races are both visible.
//!
//! Rotation keeps exactly one previous generation, `glowkey.log.1`, so the disk
//! cost is bounded at twice [`MAX_BYTES`]. Keeping one matters: a bug reported
//! just after a rotation would otherwise have no history at all, and diagnosis is
//! the only reason this file exists.
//!
//! The size is tracked **in memory**, not by asking the filesystem. GlowKey is a
//! background agent that runs for days, and the check used to happen once, when
//! the process opened the file — so a single long run grew without bound, which
//! is the bug this module had. Counting bytes as they are written costs an add on
//! a value already under the write lock, and `stat` on every keystroke is not an
//! option: the tap callback must never make a call that can wait
//! (`docs/decisions/0008`).
//!
//! Note: this records the text you type (that is the point — it is a typing engine).
//! It stays on the local machine; delete the file any time to clear it.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Rotate the log once it passes this size (5 MB). With one kept generation the
/// most GlowKey ever occupies is twice this.
const MAX_BYTES: u64 = 5_000_000;

/// The open log file, with the byte count that decides when to rotate it.
///
/// The count lives beside the handle rather than in an atomic of its own so the
/// two cannot disagree: both are only ever touched under the same lock, which
/// makes "how much is in *this* file" a single fact rather than two that drift
/// apart across a rotation.
struct LogFile {
    file: File,
    written: u64,
}

fn process_start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

fn next_seq() -> u64 {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    SEQ.fetch_add(1, Ordering::Relaxed)
}

/// The log file's path, if the platform's log directory can be resolved. Public
/// so the shell can reveal it in the file manager.
///
/// Only the location differs per platform; everything below — the rotation, the
/// byte accounting, the sequence numbers — is the same code everywhere.
#[cfg(target_os = "macos")]
pub fn path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let mut path = PathBuf::from(home);
    path.push("Library/Logs/GlowKey/glowkey.log");
    Some(path)
}

/// `%LOCALAPPDATA%\GlowKey\Logs\glowkey.log`.
#[cfg(target_os = "windows")]
pub fn path() -> Option<PathBuf> {
    let mut path = crate::platform::windows::paths::log_dir()?;
    path.push("glowkey.log");
    Some(path)
}

/// The previous generation, kept across one rotation (`glowkey.log.1`).
fn previous_path() -> Option<PathBuf> {
    let mut path = path()?;
    let name = path.file_name()?.to_owned();
    path.set_file_name(format!("{}.1", name.to_string_lossy()));
    Some(path)
}

/// Opens the log for appending and seeds the byte count from what is already
/// there, so a file grown large by *previous* runs is rotated on the next write
/// rather than being appended to forever.
fn open_appending(path: &PathBuf) -> Option<LogFile> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()?;
    let written = file.metadata().map(|m| m.len()).unwrap_or(0);
    Some(LogFile { file, written })
}

/// The shared append handle, opened once. `None` if the path or open failed (logging
/// then silently does nothing — it must never affect typing).
fn handle() -> Option<&'static Mutex<LogFile>> {
    static FILE: OnceLock<Option<Mutex<LogFile>>> = OnceLock::new();
    FILE.get_or_init(|| {
        let path = path()?;
        open_appending(&path).map(Mutex::new)
    })
    .as_ref()
}

/// Moves the current log aside and starts a new one, replacing whatever previous
/// generation was there.
///
/// A failed rename leaves the old handle in place and simply keeps appending: a
/// log that grew past its cap is a far smaller problem than logging that stops,
/// and this whole module's contract is that it can never disturb typing.
fn rotate(open: &mut LogFile, current: &PathBuf, previous: &PathBuf) {
    let _ = open.file.flush();
    if std::fs::rename(current, previous).is_err() {
        return;
    }
    if let Some(fresh) = open_appending(current) {
        *open = fresh;
    }
}

/// Appends one line to the log (and echoes to stderr when `GLOWKEY_DEBUG` is set).
/// Never panics; a logging failure is swallowed so it cannot disturb input.
/// Under `cargo test` it does nothing — tests must not write the user's real log.
pub fn log(message: &str) {
    if cfg!(test) {
        return;
    }
    let line = format!(
        "#{:<5} +{:8.3}s  {}\n",
        next_seq(),
        process_start().elapsed().as_secs_f64(),
        message
    );
    if let Some(mutex) = handle() {
        if let Ok(mut open) = mutex.lock() {
            if open.file.write_all(line.as_bytes()).is_ok() {
                open.written += line.len() as u64;
            }
            let _ = open.file.flush();
            // After the write, so a single line larger than the cap still lands
            // somewhere rather than being rotated around forever.
            if open.written > MAX_BYTES {
                if let (Some(current), Some(previous)) = (path(), previous_path()) {
                    rotate(&mut open, &current, &previous);
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// A scratch directory that removes itself, so the test never touches the
    /// user's real log.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let mut dir = std::env::temp_dir();
            dir.push(format!("glowkey-log-test-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("scratch dir");
            Self(dir)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn read(path: &PathBuf) -> String {
        let mut text = String::new();
        File::open(path)
            .expect("log exists")
            .read_to_string(&mut text)
            .expect("readable");
        text
    }

    /// Rotation moves the current log aside and starts an empty one, so the
    /// bytes already written stop counting against the cap.
    #[test]
    fn rotating_moves_the_log_aside_and_starts_empty() {
        let dir = TempDir::new("rotate");
        let current = dir.join("glowkey.log");
        let previous = dir.join("glowkey.log.1");

        let mut open = open_appending(&current).expect("open");
        open.file.write_all(b"first generation\n").expect("write");
        open.written += 17;

        rotate(&mut open, &current, &previous);

        assert_eq!(open.written, 0, "the fresh log starts the count over");
        assert_eq!(read(&previous), "first generation\n", "history is kept");
        assert_eq!(read(&current), "", "the live log starts empty");

        open.file.write_all(b"second generation\n").expect("write");
        assert_eq!(read(&current), "second generation\n");
        assert_eq!(
            read(&previous),
            "first generation\n",
            "the previous generation must not be appended to"
        );
    }

    /// A second rotation replaces the previous generation rather than
    /// accumulating: the disk cost is bounded at two files, whatever the uptime.
    #[test]
    fn only_one_previous_generation_is_kept() {
        let dir = TempDir::new("bounded");
        let current = dir.join("glowkey.log");
        let previous = dir.join("glowkey.log.1");

        let mut open = open_appending(&current).expect("open");
        open.file.write_all(b"one\n").expect("write");
        rotate(&mut open, &current, &previous);
        open.file.write_all(b"two\n").expect("write");
        rotate(&mut open, &current, &previous);

        assert_eq!(read(&previous), "two\n", "the newer generation wins");
        assert_eq!(
            std::fs::read_dir(&dir.0).expect("dir").count(),
            2,
            "never more than the live log and one previous"
        );
    }

    /// Opening seeds the count from what is already on disk, so a file grown
    /// large by earlier runs rotates on the next write instead of being appended
    /// to forever. This is the bug the module had: the size was checked once, at
    /// open, and a background agent that runs for days never re-checked.
    #[test]
    fn the_byte_count_is_seeded_from_the_existing_file() {
        let dir = TempDir::new("seed");
        let current = dir.join("glowkey.log");

        std::fs::write(&current, "already here\n").expect("seed");
        let open = open_appending(&current).expect("open");

        assert_eq!(
            open.written, 13,
            "an existing log counts toward the cap from the moment it is opened"
        );
    }
}
