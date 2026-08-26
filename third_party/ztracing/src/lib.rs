//! MIT-licensed no-op stand-in for the zed monorepo's `ztracing` crate
//! (GPL-3.0-or-later), swapped in via `[patch."https://github.com/zed-industries/zed"]`
//! so no GPL code enters the Vitre build. Provides the same public surface the
//! Apache-2.0 zed crates (sum_tree, gpui) compile against, with profiling
//! disabled: spans and events vanish, `#[instrument]` strips to the bare item.

pub use tracing::{Level, field};
pub use ztracing_macro::instrument;

/// Zero-size span handle; every operation is a no-op.
pub struct Span;

impl Span {
    pub fn current() -> Self {
        Self
    }

    pub fn enter(&self) {}

    pub fn record<T, S>(&self, _key: T, _value: S) {}
}

#[macro_export]
macro_rules! __vitre_noop_span {
    ($($tokens:tt)*) => {
        $crate::Span
    };
}

pub use __vitre_noop_span as debug_span;
pub use __vitre_noop_span as error_span;
pub use __vitre_noop_span as event;
pub use __vitre_noop_span as info_span;
pub use __vitre_noop_span as span;
pub use __vitre_noop_span as trace_span;
pub use __vitre_noop_span as warn_span;

pub fn init() {}
