//! ADR-0001 reproduction.
//!
//! Incident: PriceWhisperer US_37_08, 20 Sep 2026 — every push stream dropping, the market
//! pod killed by its liveness probe, `tracing-subscriber` panicking with "tried to clone a
//! span that already closed" in lifeguard's query spans. Root cause: a span **entered**
//! across a may yield; the coroutine resumed on another OS thread, so the exit popped the
//! wrong stack and a closed span stayed on the first thread's stack. The next contextual
//! span created on that thread was parented on it.
//!
//! This test must never be deleted; if it stops failing in the `bad` case,
//! tracing-subscriber changed and ADR-0001 needs re-reading.
//!
//! Numbers: N=64 coroutines × ITERS=400 iterations on 4 workers. The race is probabilistic;
//! at these parameters 2–4 of 64 coroutines panic on every run observed (ms02, 21 Sep
//! 2026, 3/3 runs; ~0.09 s). If it ever reports 0, raise N/ITERS before doubting ADR-0001.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const N: usize = 64;
const ITERS: usize = 400;

/// Returns (coroutines that panicked, parent mismatches in the good case).
fn workload(good: bool) -> (usize, usize) {
    may::config().set_workers(4);
    let rec = support::install();
    let mismatches = Arc::new(AtomicUsize::new(0));
    let panic_msgs = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let hs: Vec<_> = (0..N)
        .map(|i| {
            let rec = rec.clone();
            let mismatches = mismatches.clone();
            let panic_msgs = panic_msgs.clone();
            unsafe {
                may::coroutine::spawn(move || {
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                        for _ in 0..ITERS {
                            if good {
                                // explicit root: the good path never reads a thread's contextual span
                                let s = tracing::info_span!(parent: None, "good", i);
                                may_tracing::with_span(s.clone(), || {
                                    may::coroutine::yield_now();
                                    let p = may_tracing::child_span!(tracing::Level::INFO, "probe");
                                    if rec.parent_of(p.id().as_ref().unwrap()) != s.id() {
                                        mismatches.fetch_add(1, Ordering::Relaxed);
                                    }
                                });
                            } else {
                                let _e = tracing::info_span!("bad", i).entered();
                                may::coroutine::yield_now();
                                // parented on the thread's contextual span: the trap
                                let _p = tracing::info_span!("probe");
                            }
                        }
                    }));
                    match r {
                        Ok(()) => false,
                        Err(e) => {
                            let msg = e
                                .downcast_ref::<String>()
                                .cloned()
                                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                                .unwrap_or_default();
                            panic_msgs.lock().unwrap().push(msg);
                            true
                        }
                    }
                })
            }
        })
        .collect();
    let panics = hs
        .into_iter()
        .map(|h| h.join().unwrap_or(true))
        .filter(|panicked| *panicked)
        .count();
    let msgs = panic_msgs.lock().unwrap();
    if let Some(m) = msgs.first() {
        println!("first panic message: {m}");
        assert!(
            m.contains("tried to clone"),
            "unexpected panic message; not the ADR-0001 failure: {m}"
        );
    }
    (panics, mismatches.load(Ordering::Relaxed))
}

#[test]
fn entered_across_yield_panics_on_registry() {
    // Run the bad workload first so the poisoned thread stacks are its own doing; the
    // good workload never reads the contextual span, so it is immune afterwards.
    let (panics, _) = workload(false);
    println!("bad case: {panics}/{N} coroutines panicked");
    assert!(
        panics > 0,
        "the ADR-0001 failure did not reproduce; raise N/ITERS or re-read ADR-0001"
    );
    let (panics, mismatches) = workload(true);
    println!("good case: {panics} panics, {mismatches} parent mismatches");
    assert_eq!(panics, 0);
    assert_eq!(mismatches, 0);
}
