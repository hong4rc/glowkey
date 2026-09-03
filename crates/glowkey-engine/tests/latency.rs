//! A wall-clock ceiling on per-keystroke cost.
//!
//! `benches/keystroke.rs` produces the numbers; criterion's output is not an
//! assertion, so this is the guard that actually fails a build. It exists to
//! catch an order-of-magnitude regression — someone adding a dictionary lookup
//! or a regex compile to the per-key path — not to police microseconds.
//!
//! The ceiling is therefore deliberately loose. A timing assertion tight enough
//! to be interesting on a developer's machine is flaky on a shared CI runner,
//! and a flaky guard gets disabled and then rots.

use glowkey_engine::{ExclusionList, InputMethod, PlacementStyle, Session};
use std::time::Instant;

/// Keystrokes to type. Large enough that scheduler noise averages out.
const KEYSTROKES: usize = 10_000;

/// The ceiling, in microseconds per keystroke, measured at the tap's granularity.
///
/// Measured 2026-09-03 on an Apple Silicon laptop, typing `hoongf` in a loop:
/// **2 µs** per keystroke in release, **9 µs** in the unoptimised profile CI
/// actually runs. The worst single keystroke in the criterion bench (the last key
/// of `nguowif`, where the whole raw log is re-rendered and diffed against a long
/// previous render) is 2.5 µs in release. A 250 µs ceiling is roughly 28× the
/// debug figure and 125× the release one — it cannot flake on a loaded runner,
/// and it still catches anything that turns a microsecond path into a
/// millisecond one.
///
/// For scale: a fast typist produces a keystroke every 100,000 µs, and the
/// Chromium omnibox guard's accessibility round-trip is capped at 50,000 µs.
const CEILING_MICROS_PER_KEYSTROKE: u128 = 250;

/// Types the same six-key Vietnamese word over and over, committing at each
/// boundary — the ordinary path, including the auto-fix check at the commit.
#[test]
fn per_keystroke_cost_stays_in_budget() {
    let mut session = Session::new(PlacementStyle::New, ExclusionList::new());
    session.set_frontmost_app("com.apple.TextEdit");
    session.set_input_method(InputMethod::Telex);

    // Warm up, so the first-call cost of anything lazily initialised inside the
    // `vi` crate is not charged to the measurement.
    for _ in 0..100 {
        for ch in "hoongf".chars() {
            let _ = session.process_key(ch);
        }
        let _ = session.commit();
    }

    let word: Vec<char> = "hoongf".chars().collect();
    let started = Instant::now();
    let mut typed = 0usize;
    while typed < KEYSTROKES {
        for &ch in &word {
            let _ = session.process_key(ch);
            typed += 1;
        }
        let _ = session.commit();
    }
    let elapsed = started.elapsed();
    let per_key = elapsed.as_micros() / typed as u128;

    assert!(
        per_key <= CEILING_MICROS_PER_KEYSTROKE,
        "per-keystroke cost {per_key} µs exceeds the {CEILING_MICROS_PER_KEYSTROKE} µs \
         ceiling ({typed} keystrokes in {elapsed:?}). This is a hundredfold margin over \
         the measured release cost, so a failure here means something expensive entered \
         the per-key path — not that the machine was busy."
    );
    // Printed with `--nocapture`, so the number is visible when someone looks.
    println!("per-keystroke: {per_key} µs ({typed} keystrokes in {elapsed:?})");
}
