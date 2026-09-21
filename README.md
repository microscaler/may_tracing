# may_tracing

`tracing` span context for [`may`](https://github.com/microscaler/may) coroutines — the
may equivalent of Tokio's `Instrument`.

`tracing`'s "current span" is a per-OS-thread stack. A may coroutine can yield on one
thread and resume on another, so a span **entered** across a yield is exited on the wrong
thread: the exit finds nothing to pop there, and a closed span stays on the first thread's
stack. The next span created on that thread is parented on it, `tracing-subscriber`
asserts "tried to clone a span that already closed", and the coroutine unwinds. Under
load this poisons worker threads for the life of the process (PriceWhisperer,
20 Sep 2026: every push stream dropping; the market pod killed by its liveness probe).

Tokio solves the same problem by entering a task's span only while the task is polled.
may has no such hook, so this crate supplies the discipline in three layers:

| Layer | What | Where |
|---|---|---|
| 1 | **The rule**: never hold an `Entered` guard across a yield; parent spans and events explicitly | every microscaler crate on may — [ADR-0001](docs/ADR/ADR-0001-no-entered-guard-across-a-yield.md) |
| 2 | **Coroutine-scoped span context**: `with_span`, `current`, `spawn_in_current_span`, `child_span!` — the span travels with the coroutine, not the thread | this crate — [ADR-0002](docs/ADR/ADR-0002-coroutine-scoped-span-context.md), [Design](docs/Design.md) |
| 3 | **Scheduler hooks** in the may fork: enter on resume, exit on park, so vanilla `Span::current()` and `#[instrument]` work too | microscaler/may — [ADR-0003](docs/ADR/ADR-0003-scheduler-enter-exit-hooks.md) |

Layer 1 is in force now (lifeguard c9673d7 / 9e5f12d). Layer 2 is this crate. Layer 3 is
optional polish once 2 is everywhere.

## Status

0.1.0 — Epic 01 delivered (crate, tests, `otel` feature). Epic 02 (adoption in
BRRTRouter, lifeguard, pw_telemetry, PriceWhisperer) is next; see
[docs/Design.md](docs/Design.md) for the API,
[Epic 01](docs/Epics/01-span-context/README.md) and
[Epic 02](docs/Epics/02-adoption/README.md) for the stories.

## Usage

```toml
[dependencies]
may_tracing = { git = "https://github.com/microscaler/may_tracing.git", tag = "v0.1.0" }
```

The request-span shape — set, spawn with context, child span, event:

```rust
use may_tracing::BuilderExt;

// at the request boundary (BRRTRouter's service.rs): the request span becomes the
// coroutine's context. NOT entered.
let request = tracing::info_span!(parent: None, "http_request", path = %path);
let _ctx = may_tracing::set_current(request.clone());

// anywhere below: spans and events parented on it
let q = may_tracing::child_span!(tracing::Level::INFO, "execute_query", sql = %sql);
let rows = may_tracing::with_span(q, || run_query(sql));   // q is the context while it runs
may_tracing::info_in_current!(rows, "query done");

// work that belongs to the request inherits the context at spawn; work that does not is
// detached explicitly
let stream = unsafe {
    may_tracing::Builder::new().name("orders-stream".into()).stack_size(256 * 1024)
        .spawn_with_context(move || stream_loop.run())
}?;
let worker = unsafe { may_tracing::Builder::new().spawn_detached(pool_worker) }?;
```

Outside a coroutine (a Tokio worker, a test on the main thread) the same calls use a
thread-local slot, so shared code needs no `cfg`.

Tests that spawn coroutines must install their subscriber with
`tracing::subscriber::set_global_default`: `set_default` is thread-scoped and may's worker
threads never see it.

## Rules (ADR-0001)

1. Never hold an `Entered` / `EnteredSpan` guard across a point that can yield. On may,
   assume every call into lifeguard, may_minihttp, `may::sync` or `yield_now` can.
2. Create spans with an explicit parent: `child_span!` (parented on the coroutine context)
   or `span!(parent: None, ..)` for a deliberate root.
3. Record events with an explicit parent: `info_in_current!` and friends.
4. Choose inheritance at every spawn: `spawn_with_context` or `spawn_detached`. A raw
   `may::coroutine::spawn` starts with no context.

`scripts/check-span-discipline.sh` (Epic 02.5) greps for violations in CI.

## Feature flags

| Feature | Default | What |
|---|---|---|
| `otel` | off | `inject_current`, `traceparent_of_current`, `span_with_remote_parent[_str]`, `install_w3c_propagator` — W3C trace context against the coroutine context (tracing-opentelemetry 0.32 / opentelemetry 0.31) |
| `scheduler-hooks` | off | Reserved for ADR-0003; declared, empty |

## Layout

```
docs/ADR/            ADR-0001..0003
docs/Design.md       the crate design (API, storage, macros, consumers, tests, rollout)
docs/Epics/          01-span-context (this crate), 02-adoption (the consumers)
src/                 context.rs (slot, with_span), spawn.rs, macros.rs, otel.rs
tests/               support/ (recording layer, global subscriber), slot, with_span, spawn,
                     macros, regression_adr0001 (the ADR-0001 reproduction), soak (nightly), otel
examples/            request_context, otel_propagation (--features otel)
scripts/             check-span-discipline.sh (02.5)
```
