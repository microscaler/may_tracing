//! Story 01.5: child_span! and event_in_current!.

mod support;

#[test]
fn child_span_parents_on_current() {
    let rec = support::install();
    let a = tracing::info_span!("a");
    let child = may_tracing::with_span(
        a.clone(),
        || may_tracing::child_span!(tracing::Level::INFO, "child", n = 1, s = %"x", d = ?vec![1]),
    );
    assert_eq!(rec.parent_of(child.id().as_ref().unwrap()), a.id());
    assert_eq!(rec.name_of(child.id().as_ref().unwrap()), Some("child"));
    let root = may_tracing::child_span!(tracing::Level::INFO, "root");
    assert_eq!(rec.parent_of(root.id().as_ref().unwrap()), None);
}

#[test]
fn every_field_form_compiles_and_records() {
    let rec = support::install();
    let a = tracing::info_span!("a");
    may_tracing::with_span(a.clone(), || {
        let v = 3;
        let s1 = may_tracing::child_span!(tracing::Level::INFO, "f1", k = v);
        let s2 = may_tracing::child_span!(tracing::Level::INFO, "f2", k = %v);
        let s3 = may_tracing::child_span!(tracing::Level::INFO, "f3", k = ?v);
        let s4 = may_tracing::child_span!(tracing::Level::INFO, "f4", k = tracing::field::Empty);
        let s5 = may_tracing::child_span!(tracing::Level::INFO, "f5", k = v,);
        let s6 = may_tracing::child_span!(tracing::Level::INFO, "f6");
        let s7 = may_tracing::info_span_in_current!("f7", k = %v, j = ?v);
        let s8 = may_tracing::debug_span_in_current!("f8");
        let s9 = may_tracing::warn_span_in_current!("f9", k = v);
        let s10 = may_tracing::error_span_in_current!("f10");
        let s11 = may_tracing::trace_span_in_current!("f11");
        for s in [&s1, &s2, &s3, &s4, &s5, &s6, &s7, &s8, &s9, &s10, &s11] {
            assert_eq!(rec.parent_of(s.id().as_ref().unwrap()), a.id());
        }
        s4.record("k", 7);
    });
}

#[test]
fn event_in_current_attributes_to_current() {
    let rec = support::install();
    let a = tracing::info_span!("a");
    may_tracing::with_span(a.clone(), || {
        may_tracing::info_in_current!(target: "macros.t1", k = 2, "hello");
        may_tracing::event_in_current!(tracing::Level::WARN, target: "macros.t2", k = %2, "hello");
        may_tracing::warn_in_current!(target: "macros.t3", "w");
        may_tracing::error_in_current!(target: "macros.t4", e = ?"x", "e");
        may_tracing::debug_in_current!(target: "macros.t5", "d");
        may_tracing::trace_in_current!(target: "macros.t6", "t");
    });
    for t in [
        "macros.t1",
        "macros.t2",
        "macros.t3",
        "macros.t4",
        "macros.t5",
        "macros.t6",
    ] {
        assert!(rec.has_event(t), "{t} recorded");
        assert_eq!(rec.event_parent(t), a.id(), "{t} parent");
    }
    may_tracing::info_in_current!(target: "macros.root", "root event");
    assert!(rec.has_event("macros.root"));
    assert_eq!(rec.event_parent("macros.root"), None);
}

#[test]
fn works_with_no_use_statements() {
    support::install();
    may_tracing::info_in_current!("x");
    let _s = may_tracing::info_span_in_current!("y");
}
