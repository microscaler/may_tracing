// Fixture for scripts/check-span-discipline.sh: every form the check must catch.
fn b() {
    let s = tracing::info_span!("contextual"); // --strict: contextual-span
    let _e = s.enter(); // entered-span
    let _g = tracing::info_span!(parent: None, "r").entered(); // entered-span
    tracing::info!("contextual event"); // --strict: contextual-event
    s.in_scope(|| {}); // entered-span
}
fn c() {
    let _m = tracing::info_span!(
        "multiline_contextual"
    ); // --strict: contextual-span (reported on the opening line)
    tracing::warn!(
        parent: None,
        "multiline explicit is fine"
    );
}
