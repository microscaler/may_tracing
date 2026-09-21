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

Design under review; stories refined. No code yet — see [docs/Design.md](docs/Design.md)
for the API, [Epic 01](docs/Epics/01-span-context/README.md) for the crate's stories and
[Epic 02](docs/Epics/02-adoption/README.md) for adoption in BRRTRouter, lifeguard,
pw_telemetry and PriceWhisperer (executed in those repos, planned here).

## Layout

```
docs/ADR/            ADR-0001..0003
docs/Design.md       the crate design (API, storage, macros, consumers, tests, rollout)
docs/Epics/          01-span-context (this crate), 02-adoption (the consumers)
src/                 (Epic 01)
tests/               (Epic 01; the ADR-0001 reproduction lives in tests/regression_adr0001.rs)
examples/            (01.8)
scripts/             check-span-discipline.sh (02.5)
```
