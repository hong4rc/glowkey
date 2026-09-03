//! Per-keystroke cost of the engine.
//!
//! GlowKey re-derives the entire word through the `vi` crate on every key rather
//! than mutating state incrementally — the design that makes order-independent
//! tone marks work (`hoongf`, `hofong` and `hoonfg` all reach `hồng`). That
//! choice has a price, and this measures it, because an input method that adds
//! perceptible latency to typing has failed at its one job.
//!
//! Run with `cargo bench -p glowkey-engine`. The committed numbers live in the
//! phase record; the wall-clock ceiling that CI enforces is a plain test in
//! `tests/latency.rs`, since criterion's own output is not an assertion.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use glowkey_engine::{ExclusionList, InputMethod, PlacementStyle, Session};

/// A session configured the way a real user's is, in the app the ignore list
/// would leave alone.
fn session(method: InputMethod) -> Session {
    let mut session = Session::new(PlacementStyle::New, ExclusionList::new());
    session.set_frontmost_app("com.apple.TextEdit");
    session.set_input_method(method);
    session
}

/// Types a word and commits it at a boundary — one complete unit of real work,
/// including the auto-fix check that only runs at the commit.
fn type_word(session: &mut Session, word: &str) {
    for ch in word.chars() {
        let _ = session.process_key(ch);
    }
    let _ = session.commit();
}

fn words(c: &mut Criterion) {
    let mut group = c.benchmark_group("word");
    // Chosen for what each one exercises, not for length:
    //   hoongf   — the common case: two transforms and a tone mark
    //   nguowif  — the longest ordinary Vietnamese onset plus a horn and a tone
    //   vieetj   — a nặng tone on a doubled vowel
    //   ddaaij   — đ plus â plus a tone: three transformations in six keys
    //   strength — pure English, 8 letters, and the auto-fix restore fires
    for word in ["hoongf", "nguowif", "vieetj", "ddaaij", "strength"] {
        group.bench_with_input(BenchmarkId::new("telex", word), word, |b, word| {
            let mut s = session(InputMethod::Telex);
            b.iter(|| type_word(&mut s, word));
        });
    }
    // VNI carries its marks on digits, so the same syllables take different keys.
    for word in ["viet65", "hoang2", "d9a1i"] {
        group.bench_with_input(BenchmarkId::new("vni", word), word, |b, word| {
            let mut s = session(InputMethod::Vni);
            b.iter(|| type_word(&mut s, word));
        });
    }
    group.finish();
}

/// The single keystroke that costs the most: the last key of a long word, where
/// the whole raw log is re-rendered and the diff is computed against a long
/// previous render. This is the number that matters for perceived latency.
fn worst_case_keystroke(c: &mut Criterion) {
    c.bench_function("keystroke/last_key_of_nguowif", |b| {
        b.iter_batched(
            || {
                let mut s2 = session(InputMethod::Telex);
                for ch in "nguowi".chars() {
                    let _ = s2.process_key(ch);
                }
                s2
            },
            |mut primed| {
                let r = primed.process_key('f');
                std::hint::black_box(r);
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, words, worst_case_keystroke);
criterion_main!(benches);
