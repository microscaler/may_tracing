//! A may "server" coroutine sets a request span, spawns a "handler" with the context, which
//! spawns a "background" coroutine detached. Each level emits a child span and an event.
//! Run with `cargo run --example request_context`; the fmt layer prints span open/close
//! events so the linkage is visible: `execute_query` and the handler's events sit under
//! `http_request`, `housekeeping` is a root.

use may_tracing::BuilderExt;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::prelude::*;

fn background() {
    // no request context here, by choice (spawn_detached)
    let s = may_tracing::child_span!(tracing::Level::INFO, "housekeeping");
    may_tracing::with_span(s, || {
        may_tracing::info_in_current!("root-level, as intended")
    });
}

fn handler() {
    let q = may_tracing::child_span!(tracing::Level::INFO, "execute_query", sql = %"select 1");
    may::coroutine::yield_now(); // safe: nothing is entered
    may_tracing::with_span(q, || may_tracing::info_in_current!(rows = 1, "query done"));

    let bg = unsafe {
        may_tracing::Builder::new()
            .name("background".into())
            .spawn_detached(background)
    }
    .unwrap();
    bg.join().unwrap();
    may_tracing::info_in_current!("handler done");
}

fn server() {
    let request = tracing::info_span!(parent: None, "http_request", path = "/orders/stream");
    let _ctx = may_tracing::set_current(request.clone());
    may_tracing::info_in_current!("request accepted");

    let h = unsafe {
        may_tracing::Builder::new()
            .name("handler".into())
            .spawn_with_context(handler)
    }
    .unwrap();
    h.join().unwrap();
    may_tracing::info_in_current!("response sent");
}

fn main() {
    may::config().set_workers(2);
    // Coroutines only see the global subscriber (set_default is thread-scoped).
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
                .with_target(false),
        ),
    )
    .expect("subscriber");

    let s = unsafe { may::coroutine::spawn(server) };
    s.join().unwrap();
}
