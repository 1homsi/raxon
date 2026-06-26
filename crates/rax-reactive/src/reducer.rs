//! Elm/Redux-style actions + reducer pattern on top of signals.

use crate::{create_signal, Signal};

/// A reducer — takes state + action, returns new state.
pub trait Reducer: Clone + 'static {
    type Action: Clone + 'static;
    fn reduce(state: &Self, action: &Self::Action) -> Self;
}

/// A store driven by a reducer. Wraps `Signal<S>`.
#[derive(Copy, Clone)]
pub struct ReducerStore<S: Reducer> {
    signal: Signal<S>,
}

impl<S: Reducer> ReducerStore<S> {
    /// Get the current state.
    pub fn get(&self) -> S {
        self.signal.get()
    }

    /// Dispatch an action through the reducer.
    pub fn dispatch(&self, action: S::Action) {
        let current = self.signal.get();
        let next = S::reduce(&current, &action);
        self.signal.update(|s| *s = next);
    }

    /// Get the raw signal for reactive reads.
    pub fn signal(&self) -> Signal<S> {
        self.signal
    }
}

/// Create a `ReducerStore<S>` with an initial state.
pub fn use_reducer<S: Reducer>(initial: S) -> ReducerStore<S> {
    ReducerStore { signal: create_signal(initial) }
}
