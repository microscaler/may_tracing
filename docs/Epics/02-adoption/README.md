# Epic 02: Adoption — BRRTRouter, lifeguard, pw_telemetry, PriceWhisperer

## Overview

Put the context from Epic 01 into the crates that produce and consume it, in the order
that makes each step independently shippable: the producer first (BRRTRouter sets the
request span), then the library that most wants a parent (lifeguard), then the event
helper (pw_telemetry), then the one consumer with long-lived coroutines (the
PriceWhisperer push streams), then the CI rule that keeps ADR-0001 true. Each story is
**executed in its own repository**; this epic is the plan and the acceptance record, so
the sequence and the acceptance check (the 200-stream load run) live in one place.

**Status:** DONE — 21 Sep 2026 (02.1 BRRTRouter 12581dd, 02.2 lifeguard 62cfb69/0a97f63, 02.3 PriceWhisperer 8fa30ffb, 02.4 b15bd292, 02.5 v0.1.1 + CI wiring; the 200 × 40 acceptance run is recorded in PriceWhisperer US_37_08)
**Target Milestone:** the sanitisation sprint
**Dependencies:** Epic 01 v0.1.0; ADR-0001 already in force

---

## Stories

| Story | Title | Repo | Points | Status |
|-------|-------|------|--------|--------|
| [02.1](stories/02.1-brrtrouter-request-span.md) | BRRTRouter: the request span is the coroutine context, never entered | BRRTRouter | 3 | DONE |
| [02.2](stories/02.2-lifeguard-child-spans.md) | lifeguard: spans are children of the coroutine context; nesting default on | lifeguard | 2 | DONE |
| [02.3](stories/02.3-pw-telemetry-events.md) | pw_telemetry: business events attributed to the request | PriceWhisperer | 1 | DONE |
| [02.4](stories/02.4-pricewhisperer-push.md) | pricewhisperer_push: stream loops and ticks under the opening request; the load-run acceptance | PriceWhisperer | 2 | DONE |
| [02.5](stories/02.5-ci-rule.md) | The ADR-0001 grep in every may crate's CI | all four | 1 | DONE |

---

## Definition of Done (Epic-level)

- [ ] `scripts/load/streams.py --conns 200 --symbols 40 --minutes 10` against the dev
      cluster after 02.1–02.4: `pw_stream_tick_failures_total` delta 0, market pod restarts
      0, `pw_stream_disconnects_total{reason!="token_expired"}` delta 0 — recorded in
      PriceWhisperer `docs/Architecture/Push-Streams-Listen-Notify.md` §6.1 as a second table
- [ ] In OpenSearch traces (`otel-v1-apm-span-*`), a `/market/tickers/{symbol}` request
      shows `http_request` → `lifeguard.execute_query` nesting; a `/orders/stream` request
      shows `http_request` → `push.tick` → `lifeguard.execute_query`
- [ ] `lifeguard::span_nesting()` defaults to `true` and PriceWhisperer runs with it
- [ ] The grep passes in all four repos' CI with an allow-list of at most the JWKS-cache
      and shadow-mode spans in BRRTRouter (each with a `// no-yield:` note)
- [ ] The `catch_unwind` in `pricewhisperer_push`'s tick loop has not fired in a week of
      dev-cluster operation (`pw_stream_tick_failures_total` flat)

---

## Risks

| Risk | Mitigation |
|------|-----------|
| BRRTRouter's `http_request` span has other readers of `Span::current()` (RequestLogger, sanitizer, the metrics middleware) | 02.1 greps for `Span::current()` / `.enter(` in BRRTRouter before changing anything and lists every site in the story |
| lifeguard's pool worker coroutines inherit nothing and their own logging goes flat | By design (pool-scoped); 02.2 documents it and tags worker events with the pool tier instead |
| A consumer on Tokio (workers, ingestors) picks up may_tracing and the thread fallback behaves differently | 01.2's thread-slot semantics are what Tokio-side code gets; pw_telemetry's `init_worker` path is checked in 02.3 |
| The 200-stream acceptance run is disturbed by the market open | Run after 16:00 ET as US_37_08 did; the 09:30 burst run is a separate, later check |

## Delivered notes (21 Sep 2026)

- 02.1: BRRTRouter handlers are long-lived coroutines fed over a channel, not spawned per
  request, so the request span travels on a new `HandlerRequest::span` field and each
  handler loop runs the request under `may_tracing::with_span`; `spawn_with` is used
  nowhere in BRRTRouter. The three security spans were converted (created, not entered)
  rather than allow-listed, so BRRTRouter's allow-list is empty.
- 02.2: pool workers are OS threads (`thread::Builder`), so "spawn_detached" did not
  apply — worker spans are roots by construction. `MayPostgresExecutor` still had three
  `.entered()` guards that c9673d7 missed; fixed in the same commit. The histogram bucket
  fix is 0a97f63.
  Follow-up 796e219: the query span is created **on the worker thread** (`exec_on_client`),
  which has no context, so after the first deploy 0 of 1377 `lifeguard.execute_query`
  spans had a parent. The job channel now carries the enqueuing coroutine's
  `may_tracing::current()` in an `Envelope`, and the worker sets it as its thread-local
  context for the job. General lesson for the crate docs: a context does not cross a
  channel by itself; whoever hands work to another thread or coroutine must carry it
  (`spawn_with_context` for coroutines, an explicit field for channels).
- 02.4: a stream's `/stream` request span stays open for the life of the stream (the loop
  holds the handle), so its exported duration is the stream's, while `duration_ms` recorded
  by RequestLogger is the handler's. Documented, not changed.
- 02.4 acceptance (21 Sep, 10:51 ET, market open, first run with real frames): 100 × 40 for
  10 min — 67,098 frames, 0 gaps, 0 drops, 0 tick failures, 0 restarts, tail mean 4.8 ms /
  p95 ≤ 10 ms, two token-refresh storms inside; trace shape in OpenSearch is
  `http_request` → `push.tick` → `lifeguard.acquire_connection` / `lifeguard.execute_query`,
  every query span parented (15,793 / 15,793 in a 3-minute window). Getting there exposed a
  second deadlock class — an OS mutex held across a coroutine park in lifeguard's pool —
  written up as ADR-0004 and fixed in lifeguard 106c86d. The `push.tick` span is DEBUG and
  the dev profile runs `RUST_LOG=debug`, so ticks are exported in dev (~1 span/s/stream);
  set the filter to `info` before user trials or move the tick to TRACE.
- 02.5: the contextual-macro rule is `--strict` (opt-in): once nothing is entered a
  contextual span/event is mis-attribution, not the panic, and the four repos have
  thousands of contextual `debug!`/`info!` calls. Default mode (entered guards) is 0 hits
  on all four repos with no allow-list entries.
