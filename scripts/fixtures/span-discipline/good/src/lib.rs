fn a() {
    let s = may_tracing::child_span!(tracing::Level::INFO, "x");
    // no-yield: pure CPU section
    let _e = s.enter();
    let r = tracing::span!(parent: None, tracing::Level::INFO, "root");
    tracing::info!(parent: &r, "explicit");
    // root-event: startup banner on the main thread
    tracing::info!("banner");
}
