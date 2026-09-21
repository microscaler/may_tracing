# Epic 02: Adoption — BRRTRouter, lifeguard, pw_telemetry, PriceWhisperer

## Overview

Put the context from Epic 01 into the crates that produce and consume it, in the order
that makes each step independently shippable: the producer first (BRRTRouter sets the
request span), then the library that most wants a parent (lifeguard), then the event
helper (pw_telemetry), then the one consumer with long-lived coroutines (the
PriceWhisperer push streams), then the CI rule that keeps ADR-0001 true. Each story is
**executed in its own repository**; this epic is the plan and the acceptance record, so
the sequence and the acceptance check (the 200-stream load run) live in one place.

**Status:** PLANNED — blocked on Epic 01 `v0.1.0`
**Target Milestone:** the sanitisation sprint
**Dependencies:** Epic 01 v0.1.0; ADR-0001 already in force

---

## Stories

| Story | Title | Repo | Points | Status |
|-------|-------|------|--------|--------|
| [02.1](stories/02.1-brrtrouter-request-span.md) | BRRTRouter: the request span is the coroutine context, never entered | BRRTRouter | 3 | TODO |
| [02.2](stories/02.2-lifeguard-child-spans.md) | lifeguard: spans are children of the coroutine context; nesting default on | lifeguard | 2 | TODO |
| [02.3](stories/02.3-pw-telemetry-events.md) | pw_telemetry: business events attributed to the request | PriceWhisperer | 1 | TODO |
| [02.4](stories/02.4-pricewhisperer-push.md) | pricewhisperer_push: stream loops and ticks under the opening request; the load-run acceptance | PriceWhisperer | 2 | TODO |
| [02.5](stories/02.5-ci-rule.md) | The ADR-0001 grep in every may crate's CI | all four | 1 | TODO |

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
