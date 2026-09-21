# ADR-0002: A coroutine-scoped span context (`may_tracing`)

**Status**: Proposed — for review before build
**Depends on**: ADR-0001 (the rule)

## Context

ADR-0001 makes may code safe by forbidding the thread-local context. It also loses what
the context was for: a `lifeguard.execute_query` span nested under the request that ran
it, an event inside a handler attributed to that request, a stream's ticks traced under the
request that opened the stream. Explicit `parent:` arguments work when the parent is in
scope, but lifeguard cannot see BRRTRouter's request span, and threading a `&Span`
through every signature is the wrong shape for a cross-cutting concern.

Tokio's answer is that the span belongs to the *task*: `Instrumented<F>` carries the span
and enters it only for the duration of each `poll`. The unit of concurrency owns its
context; the thread is incidental. may's unit of concurrency is the coroutine, and may
has `coroutine_local!`, so the same design is available without touching the scheduler.

## Decision

A small crate, `may_tracing`, that owns **the current span of the running coroutine** in a
`coroutine_local!` slot, and exposes exactly what Tokio exposes:

| may_tracing | Tokio equivalent | Meaning |
|---|---|---|
| `with_span(span, f)` | `f.instrument(span).await` | Run `f` with `span` as this coroutine's current span; restore the previous on return (also on unwind) |
| `current() -> Span` | `Span::current()` (inside an instrumented task) | This coroutine's current span, or `Span::none()` |
| `spawn_in_current_span(builder, f)` / `.spawn_with_context(f)` | `tokio::spawn(f.in_current_span())` | The child coroutine starts with the parent's current span |
| `child_span!(level, name, fields…)` | `span!(…)` inside a task | A span whose parent is `current()` (`parent: None` when none) |
| `event_in_current!(level, …)` | `event!(…)` inside a task | An event parented on `current()` |

Semantics that matter:

- **The slot holds a `tracing::Span` handle** (`Clone + Send + Sync`; a handle is a
  ref-count on the registry entry). Holding it keeps the span *open* but not *entered*;
  nothing touches any thread's stack. The span closes when the last handle drops — for a
  request, when the handler's `with_span` scope ends and the response is written.
- **Inheritance is explicit at spawn.** A coroutine spawned through `may_tracing` gets a
  clone of the spawner's current span; one spawned through raw `may::coroutine::spawn`
  starts with `Span::none()`. This matches Tokio (`in_current_span` is opt-in) and keeps
  the cost off coroutines that don't trace. A `spawn_detached` variant exists for
  background work that must *not* be attributed to the request (the push stream loop
  after `hello`, a heal task).
- **`with_span` is re-entrant and unwind-safe**: it stores the previous value, sets the
  new one, runs `f`, restores the previous in a drop guard. Nested scopes stack
  naturally; a panic restores on the way out.
- **No `enter` anywhere.** `current()` is a read of the coroutine-local; events and child
  spans pass it as `parent:`. Code that calls `tracing::Span::current()` (the thread-local)
  still sees nothing useful — that is ADR-0003's job.
- **Runtime-agnostic fallback**: outside a may coroutine (a plain thread, a test) the
  slot is a `thread_local!`, so the same API works in workers and ingestors on Tokio
  threads.

### Who sets it

- **BRRTRouter** (`server/service.rs`): creates `http_request` and, instead of `enter`,
  wraps the dispatch in `may_tracing::with_span(span.clone(), || …)`. The handler
  coroutine is spawned with `spawn_in_current_span`, so `current()` inside a handler is
  the request span. `TracingMiddleware` records its events with `event_in_current!`.
  `RequestLogger` keeps its `span` handle for the fields it records on drop. The
  `#[handler]` macro needs no change.
- **lifeguard**: `tracing_helpers` create their spans with `parent: &may_tracing::current()`;
  the `set_span_nesting` switch from 9e5f12d becomes "read the coroutine context" and is
  safe to default **on** once BRRTRouter sets it. lifeguard's pool worker coroutines (which
  execute the query on a pooled connection) are spawned detached — the span is created in
  the caller's coroutine, so the parent is right without inheritance.
- **pw_telemetry**: `record` / `info` / `warn` / `error` emit with `parent: &current()`;
  the "standalone events only" note in its docs becomes "parented on the coroutine's span".
- **pricewhisperer_push**: `get_*_stream` handlers spawn the stream loop with
  `spawn_in_current_span` so the loop's ticks and its `bye` are children of the opening
  request; lifeguard's tail query nests under the tick's `child_span!("push.tick")`.

### Migration order

BRRTRouter first (it is the producer of the context and the largest offender of
ADR-0001), then lifeguard (flip the nesting default), then pw_telemetry, then the
services. Each step is independently shippable; nothing regresses if a later step is
delayed, because a missing context is `Span::none()` and `parent: None` is what we have
today.

## Alternatives considered

- **Thread `&Span` through signatures.** Correct, explicit, and impossible to keep
  through generated handler code, middleware and pool internals; every helper would grow
  a parameter. Rejected.
- **`tracing::dispatcher::with_default` per coroutine.** Sets the *thread's* default
  dispatcher for a closure — the same thread/coroutine confusion in a different coat, and
  it would silence other coroutines on the thread. Rejected.
- **Fix `tracing-subscriber` to keep the current-span stack per coroutine.** Would need a
  custom `Subscriber`/`Layer` that keys `CurrentSpan` by `may::coroutine::current().id()`
  instead of by thread. Possible and elegant, but it changes a dependency's core
  behaviour for every consumer of the subscriber and still requires may-awareness at
  enter/exit time (a coroutine can migrate between `enter` and `exit`, so the per-coroutine
  stack must be looked up on both). ADR-0003 is the same idea applied at the scheduler,
  where the migration is visible. Rejected for layer 2; revisited in ADR-0003.
- **Do nothing beyond ADR-0001.** Traces stay flat: every DB span a root, every event
  orphaned. Acceptable for metrics and logs, not for the trace views the hauliage and
  RERP dashboards are built on. Rejected.

## Consequences

- ~300 lines, one dependency (`may`) plus `tracing`; no unsafe beyond what `may::spawn`
  already requires.
- Nesting in traces returns, correctly, on may — and stream coroutines get a parent for
  free.
- Two explicit rules for authors: spawn through `may_tracing` when the child should be
  attributed; use `child_span!` / `event_in_current!` (or `parent: &current()`) instead of
  bare `span!` / `event!`. The ADR-0001 grep catches the `enter`s; a second grep for bare
  `tracing::span!(` without `parent:` in may crates catches the rest.
- Third-party code that relies on `Span::current()` is still flat until ADR-0003.
