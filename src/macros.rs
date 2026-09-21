//! `child_span!`, `event_in_current!` and their level sugar.
//!
//! Fields are forwarded as raw token trees, so every `tracing` field form works
//! (`k = v`, `k = %v`, `k = ?v`, `k = tracing::field::Empty`, trailing comma, none).
//! With no context the macros expand to `parent: None` — a root span / event, never a
//! contextual one, so nothing here ever reads a thread's span stack.
//!
//! A `target:` may be given right after the level, as in `tracing`
//! (`info_in_current!(target: "pw::push", "closed")`).
//!
//! There is deliberately no macro that *enters* a span.

/// A span parented on this coroutine's [`current()`](crate::current) span, or a root span
/// when there is none.
///
/// ```
/// let s = may_tracing::child_span!(tracing::Level::INFO, "execute_query", sql = %"select 1", rows = 0);
/// let _ = s;
/// ```
#[macro_export]
macro_rules! child_span {
    ($lvl:expr, target: $target:expr, $name:expr $(, $($fields:tt)*)?) => {
        match $crate::__parent_or_none() {
            ::core::option::Option::Some(parent) => {
                $crate::tracing::span!(target: $target, parent: &parent, $lvl, $name $(, $($fields)*)?)
            }
            ::core::option::Option::None => {
                $crate::tracing::span!(target: $target, parent: None, $lvl, $name $(, $($fields)*)?)
            }
        }
    };
    ($lvl:expr, $name:expr $(, $($fields:tt)*)?) => {
        match $crate::__parent_or_none() {
            ::core::option::Option::Some(parent) => {
                $crate::tracing::span!(parent: &parent, $lvl, $name $(, $($fields)*)?)
            }
            ::core::option::Option::None => {
                $crate::tracing::span!(parent: None, $lvl, $name $(, $($fields)*)?)
            }
        }
    };
}

/// [`child_span!`] at `TRACE`.
#[macro_export]
macro_rules! trace_span_in_current {
    ($name:expr $(, $($fields:tt)*)?) => {
        $crate::child_span!($crate::tracing::Level::TRACE, $name $(, $($fields)*)?)
    };
}

/// [`child_span!`] at `DEBUG`.
#[macro_export]
macro_rules! debug_span_in_current {
    ($name:expr $(, $($fields:tt)*)?) => {
        $crate::child_span!($crate::tracing::Level::DEBUG, $name $(, $($fields)*)?)
    };
}

/// [`child_span!`] at `INFO`.
#[macro_export]
macro_rules! info_span_in_current {
    ($name:expr $(, $($fields:tt)*)?) => {
        $crate::child_span!($crate::tracing::Level::INFO, $name $(, $($fields)*)?)
    };
}

/// [`child_span!`] at `WARN`.
#[macro_export]
macro_rules! warn_span_in_current {
    ($name:expr $(, $($fields:tt)*)?) => {
        $crate::child_span!($crate::tracing::Level::WARN, $name $(, $($fields)*)?)
    };
}

/// [`child_span!`] at `ERROR`.
#[macro_export]
macro_rules! error_span_in_current {
    ($name:expr $(, $($fields:tt)*)?) => {
        $crate::child_span!($crate::tracing::Level::ERROR, $name $(, $($fields)*)?)
    };
}

/// An event attributed to this coroutine's [`current()`](crate::current) span, or a root
/// event when there is none. Everything after the level is forwarded to `tracing::event!`.
///
/// ```
/// may_tracing::event_in_current!(tracing::Level::INFO, rows = 3, "query done");
/// ```
#[macro_export]
macro_rules! event_in_current {
    ($lvl:expr, target: $target:expr, $($rest:tt)*) => {
        match $crate::__parent_or_none() {
            ::core::option::Option::Some(parent) => {
                $crate::tracing::event!(target: $target, parent: &parent, $lvl, $($rest)*)
            }
            ::core::option::Option::None => {
                $crate::tracing::event!(target: $target, parent: None, $lvl, $($rest)*)
            }
        }
    };
    ($lvl:expr, $($rest:tt)*) => {
        match $crate::__parent_or_none() {
            ::core::option::Option::Some(parent) => {
                $crate::tracing::event!(parent: &parent, $lvl, $($rest)*)
            }
            ::core::option::Option::None => {
                $crate::tracing::event!(parent: None, $lvl, $($rest)*)
            }
        }
    };
}

/// [`event_in_current!`] at `TRACE`.
#[macro_export]
macro_rules! trace_in_current {
    ($($rest:tt)*) => { $crate::event_in_current!($crate::tracing::Level::TRACE, $($rest)*) };
}

/// [`event_in_current!`] at `DEBUG`.
#[macro_export]
macro_rules! debug_in_current {
    ($($rest:tt)*) => { $crate::event_in_current!($crate::tracing::Level::DEBUG, $($rest)*) };
}

/// [`event_in_current!`] at `INFO`.
///
/// ```
/// may_tracing::info_in_current!("no use statements needed");
/// ```
#[macro_export]
macro_rules! info_in_current {
    ($($rest:tt)*) => { $crate::event_in_current!($crate::tracing::Level::INFO, $($rest)*) };
}

/// [`event_in_current!`] at `WARN`.
#[macro_export]
macro_rules! warn_in_current {
    ($($rest:tt)*) => { $crate::event_in_current!($crate::tracing::Level::WARN, $($rest)*) };
}

/// [`event_in_current!`] at `ERROR`.
#[macro_export]
macro_rules! error_in_current {
    ($($rest:tt)*) => { $crate::event_in_current!($crate::tracing::Level::ERROR, $($rest)*) };
}
