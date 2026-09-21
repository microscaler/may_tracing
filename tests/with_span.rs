//! Story 01.3: with_span / set_current / ContextGuard.

mod support;

#[test]
fn nests_and_restores() {
    support::install();
    let a = tracing::info_span!("a");
    let b = tracing::info_span!("b");
    assert!(may_tracing::current().is_none());
    may_tracing::with_span(a.clone(), || {
        assert_eq!(may_tracing::current().id(), a.id());
        may_tracing::with_span(b.clone(), || {
            assert_eq!(may_tracing::current().id(), b.id())
        });
        assert_eq!(may_tracing::current().id(), a.id());
        may_tracing::with_none(|| assert!(may_tracing::current().is_none()));
        assert_eq!(may_tracing::current().id(), a.id());
    });
    assert!(may_tracing::current().is_none());
}

#[test]
fn restores_on_unwind() {
    support::install();
    let a = tracing::info_span!("a");
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        may_tracing::with_span(a, || panic!("boom"))
    }));
    assert!(r.is_err());
    assert!(may_tracing::current().is_none());
}

#[test]
fn guard_restores_on_early_return() {
    support::install();
    fn body(fail: bool) -> Result<(), ()> {
        let s = tracing::info_span!("early");
        let _g = may_tracing::set_current(s);
        if fail {
            return Err(());
        }
        Ok(())
    }
    assert!(body(true).is_err());
    assert!(may_tracing::current().is_none());
    assert!(body(false).is_ok());
    assert!(may_tracing::current().is_none());
}

#[test]
fn coroutine_and_thread_slots_are_separate() {
    support::install();
    let t = tracing::info_span!("thread");
    let _g = may_tracing::set_current(t.clone());
    let h = unsafe {
        may::coroutine::spawn(|| {
            let before = may_tracing::current().is_none();
            let c = tracing::info_span!("co");
            let _g = may_tracing::set_current(c.clone());
            (before, may_tracing::current().id() == c.id())
        })
    };
    assert_eq!(h.join().unwrap(), (true, true));
    assert_eq!(may_tracing::current().id(), t.id());
}
