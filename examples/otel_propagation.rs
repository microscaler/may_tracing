//! Two coroutines ("client" and "server") exchanging a W3C traceparent through a header
//! map; both spans land in one trace. Run with
//! `cargo run --example otel_propagation --features otel`.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};
use tracing_subscriber::prelude::*;

fn main() {
    may_tracing::install_w3c_propagator();
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("example"))),
    )
    .expect("subscriber");

    let (tx, rx) = may::sync::mpsc::channel::<std::collections::HashMap<String, String>>();

    let client = unsafe {
        may::coroutine::spawn(move || {
            let span = tracing::info_span!(parent: None, "client_call");
            may_tracing::with_span(span, || {
                let mut headers = std::collections::HashMap::new();
                may_tracing::inject_current(&mut headers);
                println!("client sends traceparent={}", headers["traceparent"]);
                tx.send(headers).unwrap();
            });
        })
    };
    let server = unsafe {
        may::coroutine::spawn(move || {
            let headers = rx.recv().unwrap();
            let span = may_tracing::span_with_remote_parent("GET /orders", &headers);
            may_tracing::with_span(span, || {
                let _q = may_tracing::child_span!(tracing::Level::INFO, "execute_query");
                may_tracing::info_in_current!("handled");
            });
        })
    };
    client.join().unwrap();
    server.join().unwrap();
    provider.force_flush().unwrap();

    for s in exporter.get_finished_spans().unwrap() {
        println!(
            "span {:<14} trace={} span={} parent={}",
            s.name,
            s.span_context.trace_id(),
            s.span_context.span_id(),
            s.parent_span_id
        );
    }
}
