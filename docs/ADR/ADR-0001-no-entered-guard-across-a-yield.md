# ADR-0001: No `Entered` guard across a yield; parents are explicit

**Status**: Accepted (in force since lifeguard c9673d7, 20 Sep 2026)
**Scope**: every microscaler crate that runs on `may` — BRRTRouter, lifeguard, pw_telemetry,
the PriceWhisperer / RERP / hauliage services and their workers

## Context

`tracing`'s current-span context lives in `tracing-subscriber`'s `Registry` as a
**per-OS-thread** stack (`CurrentSpanPerThread`). `span.enter()` pushes the span's id on
the calling thread's stack and returns an `Entered` guard; dropping the guard pops **by id,
on whichever thread the drop runs**.

A `may` coroutine is not pinned to a thread. Any blocking call in may — a channel `recv`,
socket I/O, `coroutine::sleep`, a `JoinHandle` — parks the coroutine, and the scheduler may
resume it on another worker. So this sequence, which is idiomatic in synchronous Rust and
correct on Tokio (where `Instrumented::poll` enters and exits around every poll), is
wrong on may:

```rust
let _span = tracing::info_span!("lifeguard.execute_query").entered();
let rows = self.dispatch_on_sender(...)?;   // parks; may resume on another thread
// _span dropped here: pop(id) on thread B finds nothing; thread A keeps the id
```

After the span closes, thread A's stack still names it. The next `span!` created on
thread A (any crate, any coroutine) is parented on `Span::current()` → the registry's
`clone_span(stale)` hits `assert!(refs != 0)` → **panic** in whatever coroutine happened
to be running. The thread stays poisoned; every span creation on it panics until the
stale id is popped, which nothing will do.

Observed: sporadic handler panics across the fleet for weeks; on 20 Sep 2026 under the
EPIC_37 push-stream load every stream dropping within minutes, and — once the tick loop
caught the panic — a panic-with-backtrace per tick that symbolicated a 370 MB debug
binary hundreds of times a second until the kubelet killed the pod.

Three places do this today: BRRTRouter's `http_request` span (`server/service.rs`,
entered for the life of the connection's request handling, across the dispatch to the
handler coroutine), BRRTRouter's `TracingMiddleware` (`before`/`after`, short-lived but
still entered around an event), and lifeguard's `tracing_helpers` (fixed).

## Decision

1. **A span is never entered across a yield point.** `.enter()` / `.entered()` /
   `in_scope` are allowed only around code that provably does not park: no channel ops,
   no I/O, no `sleep`, no `join`, no lifeguard call. When in doubt, don't enter.
2. **Spans and events name their parent explicitly**: `span!(parent: &p, …)`,
   `event!(parent: &p, …)`, `p.in_scope(|| event!(…))` only for non-yielding closures.
   `parent: None` when there is no context to attach to (what lifeguard does at its
   default today).
3. **Timing does not need `enter`.** An OTEL span's start is its creation and its end is
   its close (drop); fields are recorded on the handle. Entering only feeds the
   thread-local lookup. Creating the span, holding the handle across the work and dropping
   it gives the exporter the same span.
4. **Enforcement**: a CI grep in each crate for `\.entered\(\)|\.enter\(\)` in may code
   with an allow-list comment (`// no-yield: …`) on the exceptions; a `clippy`-style
   lint is a later nicety.

## Consequences

- Parent linkage becomes explicit. Where a crate cannot see its caller's span (lifeguard
  called from a handler), it creates root spans until [ADR-0002](ADR-0002-coroutine-scoped-span-context.md)
  gives it a coroutine-scoped context to read.
- `#[tracing::instrument]` and third-party crates that call `Span::current()` remain
  unsafe on may until [ADR-0003](ADR-0003-scheduler-enter-exit-hooks.md); they are not used
  in our may crates today.
- The rule is cheap to follow and removes the whole failure class; it costs nesting in
  traces until ADR-0002 lands.


## Enforcement

`scripts/check-span-discipline.sh` (may_tracing ≥ 0.1.1) fails CI on an entered guard without a `// no-yield:` note, and with `--strict` on contextual `span!`/`event!` macros without `parent:`. BRRTRouter, lifeguard, PriceWhisperer (`microservices/`, may crates) and this crate run it.
