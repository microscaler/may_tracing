# ADR-0003: Enter on resume, exit on park — scheduler hooks in the may fork

**Status**: Proposed — optional; build after ADR-0002 is deployed everywhere
**Depends on**: ADR-0002; microscaler/may (fork of `may` 0.3.51)

## Context

With ADR-0002 every microscaler crate reads the coroutine's span from `may_tracing` and
never touches the thread-local stack. Two things still see the thread-local:

1. `tracing::Span::current()` and everything built on it: `#[tracing::instrument]`, the
   implicit parent of a bare `span!` / `event!`, `tracing-opentelemetry`'s
   `Span::current().context()` used to build outbound propagation headers, and any
   third-party crate that traces "the normal way" (an HTTP client, a Redis client).
2. Code written by someone who forgot ADR-0001.

Tokio's `Instrumented::poll` makes both correct by construction: the span is on the
thread's stack exactly while the task runs on that thread. The equivalent point in may is
`coroutine_impl::run_coroutine`, the one function every resume goes through:

```rust
pub(crate) fn run_coroutine(mut co: CoroutineImpl) {
    match co.resume() {           // ← runs the coroutine on this thread until it parks
        Some(ev) => ev.subscribe(co),
        None => { /* finished or panicked */ }
    }
}
```

## Decision

Add to the may fork a **resume/park hook pair** owned by the coroutine's local data, and
have `may_tracing` install the tracing implementation of it:

- `CoroutineLocal` gains an optional `Box<dyn ResumeHook>` with two methods,
  `on_resume(&self)` and `on_park(&self)`, and a public setter usable from inside the
  coroutine (`may::coroutine::set_resume_hook(hook)`).
- `run_coroutine` calls `hook.on_resume()` immediately before `co.resume()` and
  `hook.on_park()` immediately after it returns — whichever thread it is on. The cost when
  no hook is installed is one `Option` check per resume.
- `may_tracing`'s hook: `on_resume` reads the coroutine's current span (the ADR-0002 slot)
  and calls `tracing::dispatcher::get_default(|d| d.enter(&id))` for it; `on_park` calls
  `d.exit(&id)` for the same id, before the thread can run anything else. `with_span`
  updates both the slot and, if the coroutine is currently running (it is — `with_span`
  is called from inside it), the thread's stack: exit the old id, enter the new one, and
  the reverse on scope end.

The invariant this gives is Tokio's: **a coroutine's span is on a thread's stack only
while that coroutine is executing on that thread.** Migration cannot leave a stale entry,
because the exit happens on the same thread as the enter, in the same `run_coroutine`
call. `Span::current()` inside a coroutine returns the coroutine's span.
`#[instrument]` becomes safe. The explicit `parent:` rule of ADR-0001 stays good
practice but stops being load-bearing.

## What it does not fix

- A **native thread** (not a coroutine) that enters a span and hands work to a
  coroutine — the thread's own stack is its own business; nothing migrates.
- Spans entered by a coroutine **and never exited before it finishes** — `run_coroutine`
  exits the coroutine's *context* span on park, not arbitrary spans the body entered. The
  body's `Entered` guards still drop on the coroutine's stack as before; ADR-0001 still
  applies to them. (An alternative — exit *every* id the coroutine entered — would require
  may to track a stack per coroutine; it is more machinery than the residual risk
  warrants while ADR-0001 is enforced by grep.)

## Alternatives considered

- **Per-coroutine current-span stack inside a custom subscriber layer**, keyed on
  `may::coroutine::current().id()`. Same outcome, but every `enter`/`exit`/`current` pays
  a coroutine-id lookup and a map access, and the subscriber becomes may-specific for all
  consumers. The scheduler hook is two atomics per resume and keeps `tracing-subscriber`
  stock. Rejected.
- **Upstream `may`.** The hook is small and generic (not tracing-specific), so it is a
  reasonable upstream PR to Xudong-Huang/may once it has run in the fork — the same path
  as the `may_queue` fix. Keeping it in the fork first is the decision; upstreaming is a
  follow-up.

## Consequences

- One more reason the fork exists; the diff is ~60 lines in `coroutine_impl.rs` and
  `local.rs`, behind a `tracing-hooks` feature so upstream-tracking builds are unchanged.
- Enables `#[instrument]` and stock third-party tracing on may; removes the last class of
  "forgot the rule" panics.
- Not needed for correctness once ADR-0002 is complete across our crates; that is why it
  is last, and why it is allowed to wait.
