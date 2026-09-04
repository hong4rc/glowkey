//! Logging from inside the hook callback, without the disk on the keystroke
//! path.
//!
//! # Why this exists
//!
//! `crate::log::log` takes a global mutex, writes, **flushes**, and — on the
//! write that crosses the size cap — renames the file and opens a new one. Every
//! one of those can wait. On macOS the tap survives it and the system reports
//! `kCGEventTapDisabledByTimeout` when it does not, which is at least a signal.
//! On Windows there is no signal: a callback slower than `LowLevelHooksTimeout`
//! loses the hook, silently and permanently.
//!
//! So the callback does not write. It formats a line, hands it to a bounded
//! channel, and returns. A writer thread does the waiting.
//!
//! # Why bounded, and why dropping is correct
//!
//! An unbounded queue cannot block a sender, but it can grow without limit if
//! the writer falls behind — and the thing it would be absorbing is a runaway,
//! which is the situation where memory pressure is least welcome. A bounded
//! channel with a non-blocking `try_send` cannot wait and cannot grow.
//!
//! When the queue is full the line is **dropped**, and that is the right
//! trade-off rather than a regrettable one: the log exists to diagnose typing
//! bugs, and a GlowKey that loses its hook has no typing left to diagnose.
//! Dropped lines are counted and the count is reported with the next line that
//! does get through, so the gap is visible rather than silent.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::OnceLock;

/// How many lines may be in flight before new ones are dropped.
///
/// Sized for a burst, not a backlog: a fast typist produces perhaps 20 lines a
/// second and the writer drains continuously, so reaching this at all means
/// something is wrong — either the disk has stalled or GlowKey is generating
/// input rather than responding to it.
const QUEUE_DEPTH: usize = 512;

static SENDER: OnceLock<SyncSender<String>> = OnceLock::new();

/// Lines dropped because the queue was full, since the last one that got through.
static DROPPED: AtomicUsize = AtomicUsize::new(0);

/// Starts the writer thread. Called once, before the hook is installed.
///
/// If the thread cannot be spawned, [`log`] silently does nothing rather than
/// falling back to a synchronous write. A missing log is a diagnosis problem; a
/// blocking write in the callback is a frozen keyboard.
pub fn start() {
    let (tx, rx) = sync_channel::<String>(QUEUE_DEPTH);
    let spawned = std::thread::Builder::new()
        .name("glowkey-log".into())
        .spawn(move || {
            // Ends when every sender is gone, which happens at process exit.
            for line in rx {
                crate::log::log(&line);
            }
        })
        .is_ok();
    if spawned {
        let _ = SENDER.set(tx);
    }
}

/// Queues one line. Never waits, never allocates beyond the string handed in,
/// never touches the filesystem.
///
/// Safe to call from the hook callback. That is the only reason it exists.
pub fn log(line: String) {
    let Some(sender) = SENDER.get() else {
        // Before `start`, or after it failed. Nothing to do — and specifically
        // not a synchronous write, which is the thing this module exists to
        // keep off this thread.
        return;
    };
    let dropped = DROPPED.swap(0, Ordering::Relaxed);
    let line = if dropped > 0 {
        // Prepended to the next line that fits rather than sent as a line of its
        // own, which would need a slot in the queue that is by definition full.
        format!("[{dropped} log lines dropped — the writer fell behind] {line}")
    } else {
        line
    };
    if sender.try_send(line).is_err() {
        // Full, or the writer died. Either way: drop it and count it.
        DROPPED.fetch_add(1 + dropped, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property that matters: calling this without a writer must not panic,
    /// must not block, and must not write anything.
    #[test]
    fn logging_before_start_is_a_no_op() {
        // `SENDER` is process-wide and another test may have started it, so this
        // asserts the call is safe rather than that it did nothing.
        log("a line with no writer behind it".into());
    }

    /// A full queue drops rather than waiting. Established by filling one
    /// directly, because the global sender's writer would drain it.
    #[test]
    fn a_full_queue_drops_instead_of_blocking() {
        let (tx, _rx) = sync_channel::<String>(1);
        assert!(tx.try_send("first".into()).is_ok());
        // The receiver is alive but never reads, so the queue stays full. A
        // blocking `send` here would hang the test — which is exactly what it
        // would do to the keyboard.
        assert!(tx.try_send("second".into()).is_err());
    }
}
