//! The context slot: one `Span` handle per coroutine (per thread outside a coroutine),
//! and the only two writers of it.

use std::cell::RefCell;
use std::marker::PhantomData;

use tracing::Span;

// `may::coroutine_local!` storage lives in the coroutine's local data and moves with it
// across worker threads. Outside a coroutine may's `LocalKey::with` transparently falls
// back to a thread-local backend, so this one key covers both cases (Design §2 proposed a
// second, explicit `thread_local!`; may already does that switch, keyed on
// `is_coroutine()`, so the crate does not duplicate it).
may::coroutine_local!(static CURRENT: RefCell<Span> = RefCell::new(Span::none()));

/// Run `f` with the slot that applies to the calling coroutine or thread.
///
/// Not public and not re-entrant: callers read-clone-release and never hold the borrow
/// across user code.
#[inline]
fn with_slot<R>(f: impl FnOnce(&RefCell<Span>) -> R) -> R {
    CURRENT.with(f)
}

/// `true` when called from inside a may coroutine, `false` on a native thread.
///
/// A thin wrapper over may's own detection; exposed for callers that want to know which
/// slot [`current`] is reading. Do not cache the answer — a native thread can call into
/// code that later runs in a coroutine.
#[inline]
pub fn is_in_coroutine() -> bool {
    may::coroutine::is_coroutine()
}

/// This coroutine's current span, or `Span::none()` when nothing was set.
///
/// A clone of the slot's handle: one coroutine-local lookup and an `Arc` increment on an
/// enabled span (nothing on a disabled one). Always usable as a `parent:`;
/// `.is_none()` is the "no context" test.
///
/// ```
/// assert!(may_tracing::current().is_none());
/// ```
#[inline]
pub fn current() -> Span {
    with_slot(|s| s.borrow().clone())
}

/// Restores the previous context when dropped.
///
/// Holding the guard keeps the span *open* (it is a handle), not *entered*; nothing is
/// pushed on any thread's span stack. Dropping it does not close the span; the span closes
/// when its last handle is dropped.
///
/// The guard is `!Send`: it belongs to the coroutine (or thread) whose slot it will
/// restore, and restoring somebody else's slot would be a bug.
///
/// Nested guards restore in LIFO order. Dropping guards out of source order is a caller
/// bug; what happens then is that the outer guard's restore wins.
///
/// ```compile_fail
/// let g = may_tracing::set_current(tracing::Span::none());
/// std::thread::spawn(move || drop(g));
/// ```
#[must_use = "dropping the guard restores the previous span immediately"]
pub struct ContextGuard {
    prev: Option<Span>,
    _not_send: PhantomData<*const ()>,
}

impl std::fmt::Debug for ContextGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextGuard")
            .field("prev", &self.prev)
            .finish()
    }
}

impl Drop for ContextGuard {
    fn drop(&mut self) {
        if let Some(prev) = self.prev.take() {
            // ADR-0003: with scheduler hooks installed, exit the current id on this
            // thread's stack and enter `prev` here.
            with_slot(|s| *s.borrow_mut() = prev);
        }
    }
}

/// Make `span` the current span of this coroutine (or thread) until the returned guard
/// is dropped.
///
/// The guard form of [`with_span`], for code that cannot take a closure (a function with
/// early returns, a request handler that keeps the span for its whole body):
///
/// ```
/// fn handle(ok: bool) -> Result<(), ()> {
///     let request = tracing::info_span!("http_request");
///     let _ctx = may_tracing::set_current(request.clone());
///     if !ok {
///         return Err(()); // the guard restores the previous context here too
///     }
///     may_tracing::info_in_current!("handled");
///     Ok(())
/// }
/// assert!(handle(false).is_err());
/// assert!(may_tracing::current().is_none());
/// ```
pub fn set_current(span: Span) -> ContextGuard {
    // ADR-0003: with scheduler hooks installed, exit the previous id on this thread's
    // stack and enter `span` here.
    let prev = with_slot(|s| std::mem::replace(&mut *s.borrow_mut(), span));
    ContextGuard {
        prev: Some(prev),
        _not_send: PhantomData,
    }
}

/// Run `f` with `span` as the current span; the previous value is restored when `f`
/// returns **or unwinds**. Nests.
///
/// This is the one sanctioned way to scope a span on may. It never enters the span on a
/// thread, so `f` may yield freely.
///
/// ```
/// let a = tracing::info_span!("a");
/// let seen = may_tracing::with_span(a.clone(), || may_tracing::current().id() == a.id());
/// assert!(seen);
/// assert!(may_tracing::current().is_none());
/// ```
pub fn with_span<R>(span: Span, f: impl FnOnce() -> R) -> R {
    let _g = set_current(span);
    f()
}

/// Run `f` with **no** current span — sugar for `with_span(Span::none(), f)`.
///
/// Used by [`spawn_detached`](crate::spawn_detached) and by code that must not attribute
/// a section to whatever context it happens to run under (a pool worker).
pub fn with_none<R>(f: impl FnOnce() -> R) -> R {
    with_span(Span::none(), f)
}
