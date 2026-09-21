//! `tracing` span context for [`may`] coroutines — the may equivalent of Tokio's
//! `Instrument`.
//!
//! `tracing`'s "current span" is a per-OS-thread stack. A may coroutine can yield on one
//! thread and resume on another, so a span **entered** across a yield is exited on the
//! wrong thread: the exit finds nothing to pop there, and a closed span stays on the first
//! thread's stack. The next span created on that thread is parented on it,
//! `tracing-subscriber` asserts "tried to clone a span that already closed", and the
//! coroutine unwinds. Under load this poisons worker threads for the life of the process.
//!
//! This crate keeps the current span **in the coroutine** instead: a coroutine-local slot
//! that travels with the coroutine when the scheduler moves it. Nothing here ever enters
//! a span on a thread.
//!
//! | Layer | What | Where |
//! |---|---|---|
//! | 1 | The rule: never hold an `Entered` guard across a yield; parent spans and events explicitly | ADR-0001 |
//! | 2 | Coroutine-scoped span context: [`with_span`], [`current`], [`spawn_in_current_span`], [`child_span!`] | this crate, ADR-0002 |
//! | 3 | Scheduler hooks in the may fork so vanilla `Span::current()` works too | ADR-0003 (feature `scheduler-hooks`, reserved) |
//!
//! ADRs and the design live in the repository under `docs/`.
//!
//! # The three calls a consumer needs
//!
//! ```
//! use may_tracing::BuilderExt;
//!
//! // 1. At the request boundary: make the request span the coroutine's context.
//! let request = tracing::info_span!("http_request", path = "/orders");
//! let _ctx = may_tracing::set_current(request.clone());
//!
//! // 2. Inside: spans and events parented on it, without entering anything.
//! let query = may_tracing::child_span!(tracing::Level::INFO, "execute_query", sql = %"select 1");
//! may_tracing::info_in_current!(rows = 1, "query done");
//! drop(query);
//!
//! // 3. Background work that belongs to the request inherits the context at spawn.
//! let h = unsafe {
//!     may_tracing::Builder::new()
//!         .name("orders-stream".into())
//!         .spawn_with_context(|| may_tracing::current().is_none())
//! }
//! .unwrap();
//! // (with no subscriber installed every span is disabled, hence `is_none()` here)
//! assert!(h.join().unwrap());
//! ```
//!
//! # Rules (ADR-0001)
//!
//! 1. Never hold an `Entered`/`EnteredSpan` guard across a point that can yield.
//! 2. Create spans with an explicit parent: [`child_span!`] or `span!(parent: None, ..)`.
//! 3. Record events with an explicit parent: [`event_in_current!`] and its level sugar.
//! 4. Choose inheritance at every spawn: [`BuilderExt::spawn_with_context`] or
//!    [`BuilderExt::spawn_detached`] — a raw `may::coroutine::spawn` starts with no context.
//!
//! # A note on subscribers in tests
//!
//! `tracing::subscriber::set_default` is **thread-scoped**. Coroutines run on may's worker
//! threads, which only see the *global* default subscriber, so tests that spawn coroutines
//! must install their subscriber with `set_global_default` (once per process).
//!
//! [`may`]: https://github.com/microscaler/may

#![deny(missing_docs)]
#![forbid(unsafe_op_in_unsafe_fn)]

mod context;
mod spawn;

#[macro_use]
mod macros;

#[cfg(feature = "otel")]
pub mod otel;

/// The crate version, for consumers that want to log which may_tracing they linked.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Re-export so the macros work with `$crate::` paths only (no `use tracing` at the call site).
pub use tracing;
pub use tracing::Span;

pub use context::{current, is_in_coroutine, set_current, with_none, with_span, ContextGuard};
pub use spawn::{
    go_in_current_span, spawn_detached, spawn_in_current_span, spawn_with, Builder, BuilderExt,
    JoinHandle,
};

#[cfg(feature = "otel")]
pub use otel::{
    inject_current, install_w3c_propagator, span_with_remote_parent, span_with_remote_parent_str,
    traceparent_of_current,
};

/// Macro support: the current span as `Some` when there is one, `None` otherwise.
///
/// The disabled-context branch of [`child_span!`] / [`event_in_current!`] then costs no
/// `Span::clone`.
#[doc(hidden)]
#[inline]
pub fn __parent_or_none() -> Option<Span> {
    let s = current();
    if s.is_none() {
        None
    } else {
        Some(s)
    }
}
