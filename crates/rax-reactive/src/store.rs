//! Struct-of-signals store + selector helpers.

use crate::{create_memo, create_signal, Memo, Signal};

/// A simple reactive store wrapping a single value signal.
///
/// Provides `select` to create memos over sub-fields.
///
/// # Example
/// ```
/// let store = Store::new(AppState { count: 0, name: "".to_string() });
/// let count = store.select(|s| s.count);
/// store.update(|s| s.count += 1);
/// ```
pub struct Store<S: Clone + 'static> {
    signal: Signal<S>,
}

impl<S: Clone + 'static> Store<S> {
    /// Create a new store with the given initial state.
    pub fn new(initial: S) -> Self {
        Self { signal: create_signal(initial) }
    }

    /// Get the current state (non-reactive read outside effects/memos).
    pub fn get(&self) -> S {
        self.signal.get()
    }

    /// Replace the whole state.
    pub fn set(&self, state: S)
    where
        S: PartialEq,
    {
        self.signal.set(state);
    }

    /// Update the state in-place.
    pub fn update(&self, f: impl FnOnce(&mut S)) {
        self.signal.update(f);
    }

    /// Create a derived [`Memo`] from a selector function.
    pub fn select<U: Clone + PartialEq + 'static>(
        &self,
        selector: impl Fn(&S) -> U + 'static,
    ) -> Memo<U> {
        let sig = self.signal;
        create_memo(move || selector(&sig.get()))
    }

    /// Expose the inner signal for reactive reads in effects/memos.
    pub fn signal(&self) -> Signal<S> {
        self.signal
    }
}

impl<S: Clone + PartialEq + 'static> Store<S> {
    /// Apply an optimistic update, returning a rollback closure.
    ///
    /// Call the returned closure to revert if the server confirms failure.
    pub fn optimistic_update<F>(&self, mutate: F) -> impl Fn()
    where
        F: Fn(&mut S),
    {
        let saved = self.signal.get();
        self.signal.update(|s| mutate(s));
        let rollback_sig = self.signal;
        move || rollback_sig.set(saved.clone())
    }
}

impl<S: Clone + 'static> Copy for Store<S> {}
impl<S: Clone + 'static> Clone for Store<S> {
    fn clone(&self) -> Self {
        *self
    }
}
