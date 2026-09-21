# may_tracing — design

Implements [ADR-0002](ADR/ADR-0002-coroutine-scoped-span-context.md). Read
[ADR-0001](ADR/ADR-0001-no-entered-guard-across-a-yield.md) first for why the thread-local
context is off limits on may.

## 1. Public API

```rust
pub use tracing::Span;

/// This coroutine's current span, or `Span::none()`.
pub fn current() -> Span;

/// Run `f` with `span` as the current span; the previous value is restored when
/// `f` returns or unwinds. Nests.
pub fn with_span<R>(span: Span, f: impl FnOnce() -> R) -> R;

/// A guard form of `with_span` for code that cannot take a closure (a match arm that
/// returns early). Restores on drop. NOT an `Entered`: nothing is pushed on any thread.
pub struct ContextGuard { .. }
pub fn set_current(span: Span) -> ContextGuard;

/// A span parented on `current()` (`parent: None` when there is none).
#[macro_export] macro_rules! child_span { ($lvl:expr, $name:expr $(, $($fields:tt)*)?) => … }

/// An event parented on `current()`.
#[macro_export] macro_rules! event_in_current { ($lvl:expr, $($rest:tt)*) => … }
pub use event_in_current as info_in_current /* + warn_/error_/debug_ variants */;

/// Spawn helpers. `may::coroutine::Builder` is re-exported as `Builder`.
pub unsafe fn spawn_in_current_span<F, T>(b: Builder, f: F) -> io::Result<JoinHandle<T>>
where F: FnOnce() -> T + Send + 'static, T: Send + 'static;
pub unsafe fn spawn_detached<F, T>(b: Builder, f: F) -> io::Result<JoinHandle<T>>;   // starts with Span::none()

/// Extension trait so call sites read naturally: `Builder::new().name(..).spawn_with_context(f)`.
pub trait BuilderExt { unsafe fn spawn_with_context<F, T>(self, f: F) -> io::Result<JoinHandle<T>>; unsafe fn spawn_detached<F, T>(self, f: F) -> …; }

/// Propagation helpers (feature "otel"): the W3C traceparent for `current()`, and the
/// reverse for an inbound request — thin wrappers over tracing-opentelemetry.
pub fn traceparent_of_current() -> Option<String>;
pub fn span_with_remote_parent(name: &str, traceparent: &str) -> Span;
```

The `unsafe` on the spawn helpers is may's own (`may::coroutine::spawn` is `unsafe` because
a coroutine must not block the thread); the helpers add nothing unsafe.

## 2. Storage

```rust
may::coroutine_local!(static CURRENT: RefCell<Span> = RefCell::new(Span::none()));
thread_local!(static CURRENT_THREAD: RefCell<Span> = RefCell::new(Span::none()));

fn slot<R>(f: impl FnOnce(&RefCell<Span>) -> R) -> R {
    if may::coroutine::is_coroutine() { CURRENT.with(f) } else { CURRENT_THREAD.with(f) }
}
```

`coroutine_local!` storage lives in the coroutine's local data and moves with it. A `Span`
handle is a ref-count on the registry entry: holding it keeps the span open; cloning it
is cheap; dropping the last handle closes the span (the OTEL exporter then emits it).
`Span::none()` is a zero-cost disabled span, so the slot costs nothing for coroutines that
never trace.

`is_coroutine()` makes the same API valid on native threads (Tokio workers, tests) by
falling back to a thread-local — on a native thread the thread *is* the unit of
execution, so the thread-local is correct there.

## 3. `with_span` / `set_current`

```rust
pub fn with_span<R>(span: Span, f: impl FnOnce() -> R) -> R {
    let _g = set_current(span);
    f()
}
pub fn set_current(span: Span) -> ContextGuard {
    let prev = slot(|s| std::mem::replace(&mut *s.borrow_mut(), span));
    ContextGuard { prev: Some(prev) }
}
impl Drop for ContextGuard {
    fn drop(&mut self) { if let Some(p) = self.prev.take() { slot(|s| *s.borrow_mut() = p); } }
}
```

Unwind-safe by construction (the guard's `Drop` runs during unwinding). Re-entrant:
nested guards restore in LIFO order. With ADR-0003 installed, `set_current` and the
guard's `Drop` additionally exit/enter the thread stack for the swapped ids (§7).

## 4. Spawn

```rust
pub unsafe fn spawn_in_current_span<F, T>(b: Builder, f: F) -> io::Result<JoinHandle<T>> … {
    let parent = current();                         // read in the spawner
    b.spawn(move || with_span(parent, f))          // set in the child before `f`
}
```

The clone happens on the spawner's coroutine; the child sets it as the first thing it
does. There is no window in which the child runs without its context. `spawn_detached`
is `b.spawn(move || with_span(Span::none(), f))` — explicit, so a reader knows the
detachment was chosen.

## 5. Macros

```rust
#[macro_export]
macro_rules! child_span {
    ($lvl:expr, $name:expr $(, $($fields:tt)*)?) => {{
        let __p = $crate::current();
        if __p.is_none() { ::tracing::span!(parent: None, $lvl, $name $(, $($fields)*)?) }
        else            { ::tracing::span!(parent: &__p, $lvl, $name $(, $($fields)*)?) }
    }};
}
```

`parent: &Span` on a disabled span would be treated by `tracing` as "no parent" anyway;
the branch is there so the intent is explicit and the disabled case does no work. Event
macros are the same shape around `::tracing::event!`. Fields are forwarded as raw token
trees (`$($fields:tt)*`), not as `ident = expr`: the `%x` / `?x` sigils are `tracing`
macro syntax, not expressions, and an `expr` matcher rejects them (lifeguard hit exactly
this in 9e5f12d and wrote the `execute_query` branch out by hand).

## 6. Consumers

### BRRTRouter (`server/service.rs`, `middleware/tracing.rs`)

```rust
let span = info_span!("http_request", method = %method, path = %path, …);
// was: let _enter = span.enter();
let _ctx = may_tracing::set_current(span.clone());
… // routing, auth, dispatch — handler coroutine spawned with spawn_in_current_span
```

`RequestLogger` keeps `span.clone()` for `record("status", …)` on drop, unchanged.
`TracingMiddleware::before/after`: replace the entered `http_request`/`http_response`
spans + `tracing::info!` with `may_tracing::info_in_current!(status = …, "Request completed")`
— one event on the request span instead of two throwaway spans. The JWKS cache and
shadow-mode spans (`security/*`) are short and yield-free; keep them entered with a
`// no-yield:` note, or switch to `child_span!` for consistency.

### lifeguard (`metrics::tracing_helpers`)

```rust
pub fn execute_query_span(query: &str) -> Span {
    if span_nesting() { may_tracing::child_span!(Level::INFO, "lifeguard.execute_query", query = %query) }
    else              { tracing::span!(parent: None, Level::INFO, "lifeguard.execute_query", query = %query) }
}
```

`span_nesting()` default flips to `true` in the same commit; the switch stays for a
consumer that has not adopted may_tracing. Call sites stay "created, not entered". Pool
worker coroutines are spawned with `spawn_detached` (the span is created on the caller's
side; the worker's own logging is pool-scoped, not request-scoped).

### pw_telemetry

`record` becomes `event_in_current!(level, operation = …, outcome = …, …, "pw business event")`.
The `catch_unwind` around the emit stays (belt and braces) but should never fire again.

### pricewhisperer_push

```rust
// in the handler: the loop inherits the /stream request span
may_tracing::Builder::new().name("orders-stream").stack_size(256 * 1024)
    .spawn_with_context(move || StreamLoop { … }.run());
// in StreamLoop::run_inner, per tick:
let tick = may_tracing::child_span!(Level::DEBUG, "push.tick", topic = topic.as_str());
let res = may_tracing::with_span(tick, || self.source.next(*topic, *cursor, &self.subs));
```

`with_span` (not `enter`) makes the tick span the coroutine's current span for the query,
so lifeguard's `execute_query` nests under `push.tick`, which nests under the request;
the tick span closes when `res` is back. `Frame::bye` reasons become events on the request
span (`event_in_current!`).

## 7. ADR-0003 integration (later)

Behind feature `scheduler-hooks` (requires the may fork with `tracing-hooks`):
`may_tracing::install()` sets the coroutine's `ResumeHook` on first `set_current`. The
hook's `on_resume` enters `CURRENT`'s id via `dispatcher::get_default(|d| d.enter(&id))`,
`on_park` exits it; `set_current`/`ContextGuard::drop` exit-then-enter when swapping
while running. Nothing else in the crate changes, which is the point of keeping the two
layers separate.

## 8. Tests

- `with_span` restores on return and on unwind (`catch_unwind`).
- Nested `with_span` restores LIFO.
- `spawn_in_current_span` child sees the parent's span; `spawn_detached` sees none.
- A coroutine that yields (`may::coroutine::sleep`) between `set_current` and `current()`
  still reads its own span — run under `may::config().set_workers(4)` with 200
  coroutines yielding in a loop for a second; assert every read matches the coroutine's
  own span id (this is the migration test).
- `child_span!` under `Span::none()` produces a root span; under a real span, a child
  (assert via a test `Layer` that records parent ids).
- Native-thread fallback: the same API on `std::thread::spawn`.
- **The regression**: reproduce the ADR-0001 panic with a deliberately entered span across
  a yield under a real `Registry` (expect the panic), then the same scenario through
  `with_span` (expect none). This is the test that must never be deleted.

## 9. Rollout

1. Crate with §1–§5 + §8 tests; publish as a git dependency like lifeguard.
2. BRRTRouter: `service.rs` and `middleware/tracing.rs`; run the PriceWhisperer 200-stream
   load test (`scripts/load/streams.py`) as the acceptance check — `pw_stream_tick_failures_total`
   must stay 0 and the market pod must not restart.
3. lifeguard: `child_span!` + default nesting on.
4. pw_telemetry, pricewhisperer_push, then a grep sweep of the services for bare
   `span!`/`event!` in may code.
5. ADR-0003 in the may fork, when there is a concrete consumer for `Span::current()`
   (outbound propagation headers are the likely first one).
