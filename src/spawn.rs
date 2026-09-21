//! Spawn helpers: inherit, detach, or set the span at spawn.
//!
//! Inheritance is opt-in at the spawn site. A coroutine spawned with a raw
//! `may::coroutine::spawn` starts with no context, so "why is this span an orphan" always
//! has a one-line answer.
//!
//! The `unsafe` on every function here is may's own contract — the closure must not
//! block the OS thread — and nothing else.

use std::io;

use tracing::Span;

pub use may::coroutine::{Builder, JoinHandle};

use crate::{current, with_none, with_span};

/// Spawn `f` with the **spawner's** current span as the child's context.
///
/// The parent is read on the spawner's side before `spawn` returns and set as the child's
/// first action; there is no window in which the child runs without its context, and the
/// spawner's later context changes do not leak into the child.
///
/// # Safety
///
/// May's contract for `Builder::spawn`: `f` must not block the OS thread.
pub unsafe fn spawn_in_current_span<F, T>(b: Builder, f: F) -> io::Result<JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let parent = current();
    // SAFETY: forwarded verbatim; the caller upholds may's contract.
    unsafe { b.spawn(move || with_span(parent, f)) }
}

/// Spawn `f` with **no** context, explicitly.
///
/// Functionally the same as a raw `may::coroutine::spawn`, spelled so that a reader knows
/// the detachment was chosen (a pool worker, a process-scoped listener).
///
/// # Safety
///
/// May's contract for `Builder::spawn`: `f` must not block the OS thread.
pub unsafe fn spawn_detached<F, T>(b: Builder, f: F) -> io::Result<JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    // SAFETY: forwarded verbatim; the caller upholds may's contract.
    unsafe { b.spawn(move || with_none(f)) }
}

/// Spawn `f` with an explicit `span` as the child's context, regardless of the spawner's.
///
/// For a dispatcher that knows the request span and should not have to `set_current` on
/// its own coroutine just to spawn from it.
///
/// # Safety
///
/// May's contract for `Builder::spawn`: `f` must not block the OS thread.
pub unsafe fn spawn_with<F, T>(b: Builder, span: Span, f: F) -> io::Result<JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    // SAFETY: forwarded verbatim; the caller upholds may's contract.
    unsafe { b.spawn(move || with_span(span, f)) }
}

/// [`spawn_in_current_span`] without a builder (default stack), mirroring `may::go!`.
///
/// # Safety
///
/// May's contract for `coroutine::spawn`: `f` must not block the OS thread.
pub unsafe fn go_in_current_span<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let parent = current();
    // SAFETY: forwarded verbatim; the caller upholds may's contract.
    unsafe { may::coroutine::spawn(move || with_span(parent, f)) }
}

/// The spawn helpers as methods on [`Builder`], so call sites read naturally.
///
/// ```
/// use may_tracing::BuilderExt;
///
/// let request = tracing::info_span!("http_request");
/// let h = may_tracing::with_span(request, || unsafe {
///     may_tracing::Builder::new()
///         .name("orders-stream".into())
///         .stack_size(256 * 1024)
///         .spawn_with_context(|| {
///             // the stream loop: every child_span!/info_in_current! here is under the request
///             may_tracing::is_in_coroutine()
///         })
/// })
/// .unwrap();
/// assert!(h.join().unwrap());
/// ```
pub trait BuilderExt {
    /// Spawn with the spawner's current span — see [`spawn_in_current_span`].
    ///
    /// # Safety
    ///
    /// May's contract for `Builder::spawn`: `f` must not block the OS thread.
    unsafe fn spawn_with_context<F, T>(self, f: F) -> io::Result<JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static;

    /// Spawn with no context — see [`spawn_detached`].
    ///
    /// # Safety
    ///
    /// May's contract for `Builder::spawn`: `f` must not block the OS thread.
    unsafe fn spawn_detached<F, T>(self, f: F) -> io::Result<JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static;

    /// Spawn with an explicit span — see [`spawn_with`].
    ///
    /// # Safety
    ///
    /// May's contract for `Builder::spawn`: `f` must not block the OS thread.
    unsafe fn spawn_with_span<F, T>(self, span: Span, f: F) -> io::Result<JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static;
}

impl BuilderExt for Builder {
    unsafe fn spawn_with_context<F, T>(self, f: F) -> io::Result<JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        // SAFETY: the caller upholds may's contract.
        unsafe { spawn_in_current_span(self, f) }
    }

    unsafe fn spawn_detached<F, T>(self, f: F) -> io::Result<JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        // SAFETY: the caller upholds may's contract.
        unsafe { spawn_detached(self, f) }
    }

    unsafe fn spawn_with_span<F, T>(self, span: Span, f: F) -> io::Result<JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        // SAFETY: the caller upholds may's contract.
        unsafe { spawn_with(self, span, f) }
    }
}
