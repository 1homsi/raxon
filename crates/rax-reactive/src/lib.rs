//! A fine-grained reactive runtime: the reactivity substrate for `rax`.
//!
//! # Mental model
//!
//! Three node kinds form a dependency graph inside a [`Runtime`]:
//!
//! - [`Signal<T>`] — a *source* you can [`set`](Signal::set).
//! - [`Memo<T>`] — a cached *derivation* (subscriber + source).
//! - effects (via [`create_effect`]) — *sinks* that re-run for side effects. In
//!   `rax`, an effect is what pushes a single attribute mutation to a view.
//!
//! Dependencies are tracked **automatically**: while a memo/effect runs, any
//! [`Signal::get`]/[`Memo::get`] it calls becomes an input. Conditional reads
//! work because inputs are recollected fresh on every run.
//!
//! # Glitch-freedom
//!
//! A naive "notify all dependents" scheme double-computes diamond graphs and can
//! expose stale values. We use the **Clean/Check/Dirty** pull algorithm
//! (SolidJS / "Reactively"): a write marks direct dependents `Dirty` and
//! transitive ones `Check`; reads then pull, recomputing a `Check` node only if
//! a source actually changed.
//!
//! # Runtimes & ownership
//!
//! Reactivity lives in a [`Runtime`]. Code that never names one uses a
//! per-thread default, so the simple API "just works"; multiple runtimes can
//! coexist for isolation (multi-window, inspector, test prerender). [`create_root`]
//! opens an ownership [`Scope`]: everything created inside is disposed together,
//! and a re-running effect first disposes what it created last time — no leaks.
//!
//! # Type erasure, contained
//!
//! Values are stored as `Box<dyn Any>` inside the graph (the only RTTI in the
//! framework); the public handles are fully typed, so callers never downcast.
//!
//! # Threading
//!
//! Each runtime is single-threaded (UI is single-threaded on every target).
//! Handles are `Copy`/`'static`, enabling ergonomic `move` closures. Writes from
//! other threads must be marshaled onto the UI thread by the scheduler (see the
//! architecture audit, R1/R2).

#![forbid(unsafe_code)]

mod async_derived;
mod context;
mod control;
mod effect;
mod handle;
mod history;
mod middleware;
mod persisted;
mod reducer;
mod runtime;
mod store;

pub use async_derived::{create_async_derived, create_deferred, AsyncState};
pub use context::{expect_context, provide_context, use_context};
pub use control::{batch, untrack};
pub use effect::{create_effect, Effect};
pub use handle::{create_memo, create_signal, Memo, Signal};
pub use history::{use_history, History};
pub use middleware::{add_signal_middleware, clear_signal_middlewares};
pub use persisted::{
    kv_get, kv_set, kv_set_reactive, kv_delete_reactive, watch_kv,
    persisted_bool, persisted_f64, persisted_i64, persisted_signal,
    KvNamespace,
};
pub use reducer::{use_reducer, Reducer, ReducerStore};
pub use runtime::{create_root, Runtime, Scope};
pub use store::Store;

// Alias for discoverability
pub use control::batch as transaction;
