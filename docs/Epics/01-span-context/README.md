# Epic 01: Coroutine span context

## Overview

The `may_tracing` crate as decided in [ADR-0002](../../ADR/ADR-0002-coroutine-scoped-span-context.md)
and specified in [docs/Design.md](../../Design.md): the current `tracing` span of the running
may coroutine, kept in coroutine-local storage and travelling with the coroutine across
OS threads — the may equivalent of Tokio's `Instrument`. Nothing in this crate touches the
per-thread span stack; parents are explicit and the API is Tokio's.

**Status:** PLANNED — stories refined 21 Sep 2026, for review before build
**Target Milestone:** Release 0.1.0 (git dependency for BRRTRouter, lifeguard, pw_telemetry)
**Dependencies:** `may` 0.3 (microscaler fork, unmodified for this epic), `tracing` 0.1;
ADR-0001 in force in the consumers

---

## Stories

| Story | Title | Points | Status |
|-------|-------|--------|--------|
| [01.1](stories/01.1-project-setup.md) | Project setup & Cargo.toml | 1 | TODO |
| [01.2](stories/01.2-context-slot.md) | The context slot: `current()` on coroutines and threads | 2 | TODO |
| [01.3](stories/01.3-with-span.md) | `with_span` / `set_current` / `ContextGuard` | 2 | TODO |
| [01.4](stories/01.4-spawn.md) | Spawn with inherited or detached context | 2 | TODO |
| [01.5](stories/01.5-macros.md) | `child_span!` and `event_in_current!` | 2 | TODO |
| [01.6](stories/01.6-regression-suite.md) | The regression suite: migration under load, the ADR-0001 reproduction | 3 | TODO |
| [01.7](stories/01.7-otel-propagation.md) | OTEL propagation helpers (`otel` feature) | 2 | TODO |
| [01.8](stories/01.8-docs-and-release.md) | Docs, example, release 0.1.0 | 1 | TODO |

Build order is the table order; 01.6 is written alongside 01.2–01.5 (each story lands its
own tests) and closes with the cross-cutting ones. 01.7 can slip to 0.2.0 if no consumer
needs propagation yet.

---

## Definition of Done (Epic-level)

- [ ] `cargo test` green on Linux and macOS (may's two supported hosts here)
- [ ] `cargo clippy -D warnings`, `cargo doc --no-deps` with no warnings, `cargo fmt --check`
- [ ] The regression test (01.6) reproduces the ADR-0001 panic with an entered span across
      a yield and shows `with_span` free of it, on a real `tracing_subscriber::Registry`
- [ ] Tagged `v0.1.0`; consumable as `may_tracing = { git = "https://github.com/microscaler/may_tracing.git", tag = "v0.1.0" }`
- [ ] README and rustdoc match the shipped API; `docs/Design.md` updated where the build
      diverged from the design
- [ ] No `unsafe` beyond the `may::coroutine::spawn` contract, and each `unsafe fn` says why

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

## Out of scope for this epic

- Changes to the may fork (ADR-0003) — its own epic when there is a consumer.
- Adoption in BRRTRouter, lifeguard, pw_telemetry, pricewhisperer_push — [Epic 02](../02-adoption/README.md),
  each story executed in the consumer's own repo.
- A `Layer`/`Subscriber` that keys the current span by coroutine id — rejected in ADR-0002.
