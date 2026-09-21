//! Story 01.2: the context slot.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn fresh_contexts_are_none() {
    support::install();
    assert!(may_tracing::current().is_none());
    assert!(!may_tracing::is_in_coroutine());
    let h = unsafe {
        may::coroutine::spawn(|| {
            (
                may_tracing::current().is_none(),
                may_tracing::is_in_coroutine(),
            )
        })
    };
    assert_eq!(h.join().unwrap(), (true, true));
}

/// The migration test: 200 coroutines on 4 workers, each sets its own span, yields a few
/// hundred times, and checks after every yield that current() is still its own. Also
/// asserts that coroutines really did migrate between OS threads, otherwise the test
/// proves nothing.
#[test]
fn slot_travels_with_the_coroutine_across_workers() {
    may::config().set_workers(4);
    support::install();
    let bad = Arc::new(AtomicUsize::new(0));
    let migrations = Arc::new(AtomicUsize::new(0));
    let hs: Vec<_> = (0..200)
        .map(|i| {
            let bad = bad.clone();
            let migrations = migrations.clone();
            unsafe {
                may::coroutine::spawn(move || {
                    let mine = tracing::info_span!("co", i);
                    assert!(mine.id().is_some(), "global registry must enable the span");
                    let _ctx = may_tracing::set_current(mine.clone());
                    let mut mc = support::MigrationCounter::new();
                    for _ in 0..300 {
                        may::coroutine::yield_now();
                        mc.tick();
                        if may_tracing::current().id() != mine.id() {
                            bad.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    migrations.fetch_add(mc.migrations, Ordering::Relaxed);
                })
            }
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
    assert_eq!(
        bad.load(Ordering::Relaxed),
        0,
        "context mismatches after a yield"
    );
    let m = migrations.load(Ordering::Relaxed);
    println!("thread migrations observed: {m}");
    assert!(
        m > 0,
        "no coroutine migrated between workers; the test proved nothing"
    );
}

#[test]
fn coroutines_on_one_worker_do_not_share() {
    may::config().set_workers(4);
    support::install();
    // Force both onto the same worker via Builder::id; each sets its own span and checks
    // after yielding that it still reads its own.
    let hs: Vec<_> = (0..2)
        .map(|i| unsafe {
            may::coroutine::Builder::new().id(0).spawn(move || {
                let mine = tracing::info_span!("same_worker", i);
                let _ctx = may_tracing::set_current(mine.clone());
                for _ in 0..50 {
                    may::coroutine::yield_now();
                    assert_eq!(may_tracing::current().id(), mine.id());
                }
            })
        })
        .collect();
    for h in hs {
        h.unwrap().join().unwrap();
    }
}

#[test]
fn thread_slot_is_independent() {
    support::install();
    let t = std::thread::spawn(|| {
        let s = tracing::info_span!("thread");
        let _ctx = may_tracing::set_current(s.clone());
        may_tracing::current().id() == s.id()
    });
    assert!(t.join().unwrap());
    assert!(may_tracing::current().is_none(), "main thread untouched");
}
