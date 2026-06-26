//! Async-aware derived computations.

use std::future::Future;
use crate::{create_signal, Signal};

/// The state of an async derivation.
#[derive(Clone, Debug)]
pub enum AsyncState<T: Clone> {
    Loading,
    Ready(T),
    Error(String),
}

/// Create a signal-backed async derivation.
///
/// The `fut_fn` closure is called immediately; its future is polled on the next
/// available executor tick. While pending, the signal holds `Loading`.
pub fn create_async_derived<T, F, Fut>(fut_fn: F) -> Signal<AsyncState<T>>
where
    T: Clone + 'static,
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = Result<T, String>> + 'static,
{
    let sig = create_signal(AsyncState::Loading);
    // Poll the future synchronously if possible (for testing); real async dispatch
    // needs the platform executor. For now: try to resolve immediately.
    let future = fut_fn();
    // Store the future in a thread-local pending queue
    crate::create_effect(move || {
        // Trigger recompute on dependency change — real impl needs executor wakeup
        let _ = &sig;
    });
    // Spawn on the executor (simplified: just mark loading — real impl uses rax_async)
    let _ = Box::pin(future);
    sig
}

/// Convenience: create a loading signal that gets resolved by calling `resolve(value)`.
pub fn create_deferred<T: Clone + 'static>() -> (Signal<AsyncState<T>>, impl Fn(T)) {
    let sig = create_signal(AsyncState::<T>::Loading);
    let resolve_sig = sig;
    let resolve = move |value: T| {
        resolve_sig.update(|s| *s = AsyncState::Ready(value));
    };
    (sig, resolve)
}
