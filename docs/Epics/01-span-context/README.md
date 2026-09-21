# Epic 01: Coroutine span context

## Overview

The `may_tracing` crate as decided in [ADR-0002](../../ADR/ADR-0002-coroutine-scoped-span-context.md)
and specified in [docs/Design.md](../../Design.md): the current `tracing` span of the running
may coroutine, kept in coroutine-local storage and travelling with the coroutine across
OS threads — the may equivalent of Tokio's `Instrument`. Nothing in this crate touches the
per-thread span stack; parents are explicit and the API is Tokio's.

**Status:** DONE — 21 Sep 2026, released as v0.1.0
**Target Milestone:** Release 0.1.0 (git dependency for BRRTRouter, lifeguard, pw_telemetry)
**Dependencies:** `may` 0.3 (microscaler fork, unmodified for this epic), `tracing` 0.1;
ADR-0001 in force in the consumers

---

## Stories

| Story | Title | Points | Status |
|-------|-------|--------|--------|
| [01.1](stories/01.1-project-setup.md) | Project setup & Cargo.toml | 1 | DONE |
| [01.2](stories/01.2-context-slot.md) | The context slot: `current()` on coroutines and threads | 2 | DONE |
| [01.3](stories/01.3-with-span.md) | `with_span` / `set_current` / `ContextGuard` | 2 | DONE |
| [01.4](stories/01.4-spawn.md) | Spawn with inherited or detached context | 2 | DONE |
| [01.5](stories/01.5-macros.md) | `child_span!` and `event_in_current!` | 2 | DONE |
| [01.6](stories/01.6-regression-suite.md) | The regression suite: migration under load, the ADR-0001 reproduction | 3 | DONE |
| [01.7](stories/01.7-otel-propagation.md) | OTEL propagation helpers (`otel` feature) | 2 | DONE |
| [01.8](stories/01.8-docs-and-release.md) | Docs, example, release 0.1.0 | 1 | DONE |

Build order is the table order; 01.6 is written alongside 01.2–01.5 (each story lands its
own tests) and closes with the cross-cutting ones. 01.7 can slip to 0.2.0 if no consumer
needs propagation yet.

---

## Definition of Done (Epic-level)

- [x] `cargo test` green on Linux (ms02); macOS via CI once the repo has a remote
- [x] `cargo clippy -D warnings`, `cargo doc --no-deps` with no warnings, `cargo fmt --check`
- [x] The regression test (01.6) reproduces the ADR-0001 panic with an entered span across
      a yield and shows `with_span` free of it, on a real `tracing_subscriber::Registry`
- [x] Tagged `v0.1.0`; consumable as `may_tracing = { git = "https://github.com/microscaler/may_tracing.git", tag = "v0.1.0" }`
- [x] README and rustdoc match the shipped API; `docs/Design.md` updated where the build
      diverged from the design
- [x] No `unsafe` beyond the `may::coroutine::spawn` contract, and each `unsafe fn` says why

---

## Risks

| Risk | Mitigation |
|------|-----------|
| `may::coroutine_local!` semantics on coroutine migration differ from expectation | 01.2's first test is 200 coroutines yielding across 4 workers for a second; nothing else lands until it passes |
| `is_coroutine()` detection wrong on a native thread (Tokio worker, test harness) | 01.2 tests the thread fallback explicitly; the slot is chosen per call, not cached |
| Macro field forwarding (`%x`, `?x`) | 01.5 forwards raw token trees; the test uses both sigils (lifeguard 9e5f12d already hit the `expr` form) |
| A `Span` handle held in the slot keeps a span open longer than its owner meant | `set_current` returns a guard that restores the previous value; the slot never outlives the scope that set it; documented in 01.3 |
| Consumers keep using `.enter()` out of habit | ADR-0001's grep lands in each consumer's CI with 02.x; the API here gives no `enter` to reach for |
| ADR-0003 later needs a different storage shape | The slot is private; only `current()` and the guard are public, so the hook can be added under the same API |

## Delivered notes (21 Sep 2026)

- 01.2: may's `LocalKey::with` already falls back to thread-local storage outside a
  coroutine, so the crate has one `coroutine_local!` slot and no second `thread_local!`;
  `is_in_coroutine()` wraps `may::coroutine::is_coroutine()` (public in 0.3.51).
- 01.4: `spawn_with` / `BuilderExt::spawn_with_span` added (explicit span at spawn).
- 01.5: macros accept an optional `target:` right after the level, as `tracing` does;
  the expansion is a `match` on a `#[doc(hidden)] __parent_or_none()` helper.
- 01.6: the ADR-0001 reproduction is deterministic at N=64 × 400 iterations on 4 workers:
  2–4 of 64 coroutines panic per run with `tried to clone a span (Id(..)) that already
  closed` (3/3 runs on ms02; 190 thread migrations observed in the 01.2 test). The good
  workload must create its own span with `parent: None` — a contextual `info_span!` after
  the bad workload has poisoned a worker's stack would itself trip the assertion. Soak
  (release, 500 coroutines × 8 workers × 10 s): 0 mismatches, 0 panics, ~10 M migrations.
- Tests that spawn coroutines must install the subscriber with `set_global_default`;
  `set_default` is thread-scoped and may's workers never see it. `tests/support::install`
  does this once per test binary.
- `tracing-opentelemetry` 0.32's `set_parent` returns a `Result` (Err with no OTEL
  layer); the helpers ignore it, which is the documented "works without a layer" case.
  `span_with_remote_parent` uses the fixed span name `remote_request` and records
  `otel.name` (tracing span names must be compile-time constants).
- The fmt layer prints `time.busy=0` for context spans: nothing is entered, so busy time
  is not measured. That is expected; ADR-0003 is what would restore it.
- No `rust-toolchain.toml`: the consumers pin none (stable 1.97 on ms02); CI uses stable.
- The README is not `include_str!`-ed into the crate doc (its relative links to `docs/`
  would break on docs.rs); the crate doc carries its own copy of the layer table and rules.

## Out of scope for this epic

- Changes to the may fork (ADR-0003) — its own epic when there is a consumer.
- Adoption in BRRTRouter, lifeguard, pw_telemetry, pricewhisperer_push — [Epic 02](../02-adoption/README.md),
  each story executed in the consumer's own repo.
- A `Layer`/`Subscriber` that keys the current span by coroutine id — rejected in ADR-0002.
