//! Story 01.7: traceparent in/out against the coroutine context.
#![cfg(feature = "otel")]

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};
use tracing_subscriber::prelude::*;

/// One global subscriber per process (worker threads only see the global default).
fn install() -> (InMemorySpanExporter, SdkTracerProvider) {
    static G: std::sync::OnceLock<(InMemorySpanExporter, SdkTracerProvider)> =
        std::sync::OnceLock::new();
    G.get_or_init(|| {
        may_tracing::install_w3c_propagator();
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let layer = tracing_opentelemetry::layer().with_tracer(provider.tracer("may_tracing-test"));
        tracing::subscriber::set_global_default(tracing_subscriber::registry().with(layer))
            .expect("installed once");
        (exporter, provider)
    })
    .clone()
}

#[test]
fn traceparent_of_current_is_none_without_context() {
    install();
    assert_eq!(may_tracing::traceparent_of_current(), None);
}

#[test]
fn round_trip_shares_trace_id() {
    let (exporter, provider) = install();
    let mut carrier = std::collections::HashMap::<String, String>::new();
    let client = tracing::info_span!("client");
    let tp = may_tracing::with_span(client.clone(), || {
        may_tracing::inject_current(&mut carrier);
        may_tracing::traceparent_of_current()
    });
    let tp = tp.expect("traceparent under an OTEL layer");
    assert!(tp.starts_with("00-"), "{tp}");
    assert_eq!(carrier.get("traceparent"), Some(&tp));

    // "another process": a coroutine on a worker thread extracts and handles
    let h = unsafe {
        may::coroutine::spawn(move || {
            let server = may_tracing::span_with_remote_parent("GET /orders", &carrier);
            may_tracing::with_span(server, || {
                let child = may_tracing::child_span!(tracing::Level::INFO, "server_child");
                drop(child);
            });
        })
    };
    h.join().unwrap();
    drop(client);
    provider.force_flush().unwrap();

    // tests in this binary run in parallel and share the exporter: keep our spans only
    let spans: Vec<_> = exporter
        .get_finished_spans()
        .unwrap()
        .into_iter()
        .filter(|s| ["client", "GET /orders", "server_child"].contains(&s.name.as_ref()))
        .collect();
    assert_eq!(spans.len(), 3, "got {} spans", spans.len());
    let trace_ids: std::collections::HashSet<_> =
        spans.iter().map(|s| s.span_context.trace_id()).collect();
    assert_eq!(trace_ids.len(), 1, "all spans share one trace id");
    assert!(
        spans.iter().any(|s| s.name == "GET /orders"),
        "otel.name applied"
    );
}

#[test]
fn str_form_round_trip() {
    let (exporter, provider) = install();
    let client = tracing::info_span!("client2");
    let tp = may_tracing::with_span(client.clone(), may_tracing::traceparent_of_current).unwrap();
    let server = may_tracing::span_with_remote_parent_str("srv", &tp, None);
    drop(server);
    drop(client);
    provider.force_flush().unwrap();
    let spans = exporter.get_finished_spans().unwrap();
    let c = spans.iter().find(|s| s.name == "client2").unwrap();
    let s = spans.iter().find(|s| s.name == "srv").unwrap();
    assert_eq!(c.span_context.trace_id(), s.span_context.trace_id());
    assert_eq!(s.parent_span_id, c.span_context.span_id());
}
