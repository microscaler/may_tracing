# Changelog

All notable changes to this project will be documented in this file. The format is based
on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.1] - 2026-09-21

### Added

- `scripts/check-span-discipline.sh` (Epic 02.5): the ADR-0001 grep. Fails on `.enter()` /
  `.entered()` / `.in_scope(` without a `// no-yield:` note; `--strict` also fails on
  contextual `span!` / `event!` macros without `parent:` (opt-in: mis-attribution, not
  the panic). Fixtures under `scripts/fixtures/span-discipline/`, tested by
  `tests/check_span_discipline.rs`; wired into BRRTRouter, lifeguard and PriceWhisperer CI
  via the fetched git checkout. Library code unchanged from 0.1.0.

## [0.1.0] - 2026-09-21

### Added

- `current()`, `set_current()` / `ContextGuard`, `with_span()`, `with_none()`: a span
  context stored per **coroutine** (may `coroutine_local!`, thread-local fallback outside a
  coroutine) that never touches a thread's span stack (ADR-0002).
- `spawn_in_current_span`, `spawn_detached`, `spawn_with`, `go_in_current_span` and the
  `BuilderExt` trait (`spawn_with_context`, `spawn_detached`, `spawn_with_span`).
- `child_span!` / `event_in_current!` with level sugar (`info_span_in_current!`,
  `info_in_current!`, …); fields forwarded as token trees so `%x` / `?x` work.
- Feature `otel`: `inject_current`, `traceparent_of_current`, `span_with_remote_parent`,
  `span_with_remote_parent_str`, `install_w3c_propagator`.
- Feature `scheduler-hooks`: reserved for ADR-0003, intentionally empty.
- The ADR-0001 reproduction (`tests/regression_adr0001.rs`) and the nightly soak.
