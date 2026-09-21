# ADR-0004: No OS-thread lock across a yield

**Status:** Accepted — 21 Sep 2026
**Context:** microscaler crates on `may` (BRRTRouter, lifeguard, may_minihttp, PriceWhisperer)

## Context

ADR-0001 forbids holding a `tracing` `Entered` guard across a point that can yield, because
the guard is per OS thread and the coroutine may resume on another. The same shape of bug
exists for any **OS-thread blocking primitive** held across a yield — `std::sync::Mutex`,
`RwLock`, `Condvar`, a blocking channel send — with a worse failure: not a panic, a
deadlock.

On 21 Sep 2026 the PriceWhisperer market service wedged three times under a 100-stream open
storm with the CPU idle. lifeguard's pool took a per-slot `std::sync::Mutex` in
`WorkerPool::dispatch` and held it across `reply_rx.recv()` — a coroutine park. Each
same-slot caller blocked its `may` worker thread on the futex while the holder sat parked
waiting for its DB reply; once enough of the 49 workers were blocked, the holder could never
be resumed to release the lock. `/health` timed out, the liveness probe restarted the pod
every two minutes. It had never fired on a quiet market because a 0-row tick completes in
well under a millisecond and the window was too small to fill.

## Decision

1. Code that runs on `may` coroutines must not hold an OS-thread lock or block on an
   OS-thread primitive across a point that can yield (`may::sync`, `may_postgres`,
   `may_minihttp`, `yield_now`, a channel `recv`). Use `may::sync::{Mutex, RwLock, mpsc}`
   for anything held across such a call; a waiter then parks its coroutine and frees the
   worker thread.
2. A `std::sync::Mutex` is acceptable only for a critical section that is provably
   yield-free and short (a counter, a map insert); annotate the site `// no-yield:` as for
   ADR-0001.
3. A pool, queue or executor that hands work to OS threads and waits for the answer waits
   with a `may`-aware primitive on the coroutine side (lifeguard: `may::sync::mpsc` for the
   reply, `may::sync::Mutex` for the slot).

## Consequences

- lifeguard 106c86d: slot locks are `may::sync::Mutex`.
- `scripts/check-span-discipline.sh` does not detect this class (a lock is not a span);
  a `--locks` mode that flags `std::sync::{Mutex,RwLock,Condvar}` in may crates without a
  `// no-yield:` note is the natural follow-up.
- When a `may` service shows every request hanging with the CPU idle, look for worker
  threads in `futex_wait` under a `lock` frame before anything else; `kubectl debug
  --profile=sysadmin` + `eu-stack -p 1` gets the stacks in under a minute (gdb OOMs on a
  debug binary at the pod's memory limit).
