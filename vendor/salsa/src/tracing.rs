//! Wrappers around `tracing` macros.
//!
//! `DEBUG` and `TRACE` events are enabled only with the `detailed-trace` feature. This gating
//! applies only to events; spans must be gated at their call sites. The wrappers also avoid
//! inlining most tracing machinery into hot paths.

macro_rules! trace {
    ($($x:tt)*) => {{
        if cfg!(feature = "detailed-trace") {
            crate::tracing::event!(TRACE, $($x)*)
        }
    }};
}

macro_rules! warn_event {
    ($($x:tt)*) => {
        crate::tracing::event!(WARN, $($x)*)
    };
}

macro_rules! info {
    ($($x:tt)*) => {
        crate::tracing::event!(INFO, $($x)*)
    };
}

macro_rules! debug {
    ($($x:tt)*) => {{
        if cfg!(feature = "detailed-trace") {
            crate::tracing::event!(DEBUG, $($x)*)
        }
    }};
}

#[allow(unused_macros)]
macro_rules! debug_span {
    ($($x:tt)*) => {
        crate::tracing::span!(DEBUG, $($x)*)
    };
}

#[expect(unused_macros)]
macro_rules! info_span {
    ($($x:tt)*) => {
        crate::tracing::span!(INFO, $($x)*)
    };
}

macro_rules! event {
    ($level:ident, $($x:tt)*) => {{
        let event = {
            #[cold] #[inline(never)] || { ::tracing::event!(::tracing::Level::$level, $($x)*) }
        };

        if ::tracing::enabled!(::tracing::Level::$level) {
            event();
        }
    }};
}

#[allow(unused_macros)]
macro_rules! span {
    ($level:ident, $($x:tt)*) => {{
        let span = {
            #[cold] #[inline(never)] || { ::tracing::span!(::tracing::Level::$level, $($x)*) }
        };

        if ::tracing::enabled!(::tracing::Level::$level) {
            span()
        } else {
            ::tracing::Span::none()
        }
    }};
}

#[expect(unused_imports)]
pub(crate) use {debug, debug_span, event, info, info_span, span, trace, warn_event as warn};
