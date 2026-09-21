//! Nightly soak (`cargo test --release --test soak -- --ignored`): mixed workload on 8
//! workers for 10 s; zero context mismatches, zero panics, migrations > 0.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use may_tracing::BuilderExt;

#[test]
#[ignore]
fn soak_mixed_workload() {
    may::config().set_workers(8);
    let rec = support::install();
    let mismatches = Arc::new(AtomicUsize::new(0));
    let migrations = Arc::new(AtomicUsize::new(0));
    let panics = Arc::new(AtomicUsize::new(0));
    let deadline = Instant::now() + Duration::from_secs(10);
    let hs: Vec<_> = (0..500)
        .map(|i| {
            let rec = rec.clone();
            let mismatches = mismatches.clone();
            let migrations = migrations.clone();
            let panics = panics.clone();
            unsafe {
                may::coroutine::spawn(move || {
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let mine = tracing::info_span!("soak", i);
                        let _ctx = may_tracing::set_current(mine.clone());
                        let mut mc = support::MigrationCounter::new();
                        let mut n = 0usize;
                        while Instant::now() < deadline {
                            n += 1;
                            match n % 4 {
                                0 => {
                                    let c = may_tracing::child_span!(tracing::Level::INFO, "c");
                                    if rec.parent_of(c.id().as_ref().unwrap()) != mine.id() {
                                        mismatches.fetch_add(1, Ordering::Relaxed);
                                    }
                                }
                                1 => may_tracing::info_in_current!(target: "soak.ev", "e"),
                                2 => {
                                    let h = may_tracing::Builder::new()
                                        .spawn_with_context(|| may_tracing::current().id())
                                        .unwrap();
                                    if h.join().unwrap() != mine.id() {
                                        mismatches.fetch_add(1, Ordering::Relaxed);
                                    }
                                }
                                _ => may::coroutine::yield_now(),
                            }
                            mc.tick();
                            if may_tracing::current().id() != mine.id() {
                                mismatches.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        migrations.fetch_add(mc.migrations, Ordering::Relaxed);
                    }));
                    if r.is_err() {
                        panics.fetch_add(1, Ordering::Relaxed);
                    }
                })
            }
        })
        .collect();
    for h in hs {
        let _ = h.join();
    }
    println!(
        "soak: mismatches={} panics={} migrations={}",
        mismatches.load(Ordering::Relaxed),
        panics.load(Ordering::Relaxed),
        migrations.load(Ordering::Relaxed)
    );
    assert_eq!(mismatches.load(Ordering::Relaxed), 0);
    assert_eq!(panics.load(Ordering::Relaxed), 0);
    assert!(migrations.load(Ordering::Relaxed) > 0);
}
