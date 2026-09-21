//! Story 01.4: spawn with inherited, detached or explicit context.

mod support;

use may_tracing::BuilderExt;

#[test]
fn child_inherits_or_not() {
    may::config().set_workers(4);
    support::install();
    let a = tracing::info_span!("a");
    let b = tracing::info_span!("b");
    may_tracing::with_span(a.clone(), || {
        let inherited = unsafe {
            may_tracing::Builder::new().spawn_with_context(|| may_tracing::current().id())
        }
        .unwrap();
        let detached = unsafe {
            may_tracing::Builder::new().spawn_detached(|| may_tracing::current().is_none())
        }
        .unwrap();
        let explicit = unsafe {
            may_tracing::Builder::new().spawn_with_span(b.clone(), || may_tracing::current().id())
        }
        .unwrap();
        let go = unsafe { may_tracing::go_in_current_span(|| may_tracing::current().id()) };
        let raw = unsafe { may::coroutine::spawn(|| may_tracing::current().is_none()) };
        assert_eq!(inherited.join().unwrap(), a.id());
        assert!(detached.join().unwrap());
        assert_eq!(explicit.join().unwrap(), b.id());
        assert_eq!(go.join().unwrap(), a.id());
        assert!(
            raw.join().unwrap(),
            "a raw spawn starts with no context: the opt-in contract"
        );
    });
}

#[test]
fn contexts_are_independent_after_spawn() {
    support::install();
    let a = tracing::info_span!("a");
    let c = tracing::info_span!("c");
    let c_id = c.id();
    let (tx, rx) = may::sync::mpsc::channel::<()>();
    let h = may_tracing::with_span(a.clone(), || unsafe {
        may_tracing::Builder::new().spawn_with_context(move || {
            let _ = rx.recv(); // wait until the spawner has changed its own context
            let inherited = may_tracing::current().id();
            let _g = may_tracing::set_current(tracing::info_span!("child_own"));
            inherited
        })
    })
    .unwrap();
    // the spawner moves on to another context; the child must still see `a`
    let _g = may_tracing::set_current(c);
    tx.send(()).unwrap();
    assert_eq!(h.join().unwrap(), a.id());
    // and the child's set_current did not touch the spawner
    assert_eq!(may_tracing::current().id(), c_id);
    drop(_g);
    assert!(may_tracing::current().is_none());
}
