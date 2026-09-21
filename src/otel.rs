//! W3C trace-context propagation against the coroutine context (feature `otel`).
//!
//! `tracing-opentelemetry` propagates through `Span::current()` — the thread-local — which
//! is exactly what must not be read on may. These helpers do the same two conversions
//! against [`current()`](crate::current):
//!
//! * outbound: [`inject_current`] / [`traceparent_of_current`] — the current span's OTEL
//!   context into a carrier (headers);
//! * inbound: [`span_with_remote_parent`] / [`span_with_remote_parent_str`] — a new root
//!   span linked to the remote context found in a carrier.
//!
//! The carrier forms use the **global** propagator
//! (`opentelemetry::global::get_text_map_propagator`) so a consumer's choice is honoured.
//! OpenTelemetry's default global propagator is a no-op: a process that has not installed
//! one gets nothing injected and nothing extracted. Call [`install_w3c_propagator`] once
//! at startup, or set your own. The string forms are W3C by definition and use
//! `TraceContextPropagator` directly.
//!
//! There is no `Span::current()` anywhere in this module.

use std::collections::HashMap;

use opentelemetry::propagation::{Extractor, Injector, TextMapPropagator};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::current;

/// Install the W3C `TraceContextPropagator` as the global text-map propagator.
///
/// Idempotent in effect; call it once at process start (BRRTRouter's telemetry init is
/// the natural place).
pub fn install_w3c_propagator() {
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
}

/// Inject the current span's trace context (`traceparent` + `tracestate`) into `carrier`
/// using the global propagator. Does nothing when there is no current span.
///
/// ```
/// let mut headers = std::collections::HashMap::<String, String>::new();
/// may_tracing::inject_current(&mut headers);
/// // with no OTEL layer / no context installed, nothing is injected
/// assert!(headers.is_empty());
/// ```
pub fn inject_current(carrier: &mut dyn Injector) {
    let span = current();
    if span.is_none() {
        return;
    }
    let cx = span.context();
    opentelemetry::global::get_text_map_propagator(|p| p.inject_context(&cx, carrier));
}

/// The W3C `traceparent` header value for the current span, or `None` when there is no
/// current span or it has no OTEL context (no `tracing_opentelemetry` layer installed).
///
/// Uses `TraceContextPropagator` directly — the header is W3C by definition — so it does
/// not depend on the global propagator.
pub fn traceparent_of_current() -> Option<String> {
    let span = current();
    if span.is_none() {
        return None;
    }
    let cx = span.context();
    let mut carrier: HashMap<String, String> = HashMap::new();
    TraceContextPropagator::new().inject_context(&cx, &mut carrier);
    carrier.remove("traceparent")
}

/// A new **root** span named `name`, parented on the remote trace context found in
/// `carrier` (via the global propagator). When the carrier holds no context the span is a
/// plain root span; either way it is usable as a `parent:` and as a context for
/// [`with_span`](crate::with_span).
///
/// The span's `tracing` name is `remote_request`; `name` is recorded as `otel.name`, which
/// `tracing-opentelemetry` uses as the exported span name. (`tracing` span names must be
/// compile-time constants.)
///
/// ```
/// let headers = std::collections::HashMap::<String, String>::new();
/// let server = may_tracing::span_with_remote_parent("GET /orders", &headers);
/// may_tracing::with_span(server, || {
///     may_tracing::info_in_current!("handling");
/// });
/// ```
pub fn span_with_remote_parent(name: &str, carrier: &dyn Extractor) -> Span {
    let cx = opentelemetry::global::get_text_map_propagator(|p| p.extract(carrier));
    let span = tracing::span!(
        parent: None,
        tracing::Level::INFO,
        "remote_request",
        otel.name = name
    );
    let _ = span.set_parent(cx);
    span
}

/// [`span_with_remote_parent`] for callers holding the header strings, using the W3C
/// `TraceContextPropagator` directly.
pub fn span_with_remote_parent_str(
    name: &str,
    traceparent: &str,
    tracestate: Option<&str>,
) -> Span {
    let mut carrier: HashMap<String, String> = HashMap::new();
    carrier.insert("traceparent".to_owned(), traceparent.to_owned());
    if let Some(ts) = tracestate {
        carrier.insert("tracestate".to_owned(), ts.to_owned());
    }
    let cx = TraceContextPropagator::new().extract(&carrier);
    let span = tracing::span!(
        parent: None,
        tracing::Level::INFO,
        "remote_request",
        otel.name = name
    );
    let _ = span.set_parent(cx);
    span
}
