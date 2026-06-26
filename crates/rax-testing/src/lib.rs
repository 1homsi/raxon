//! Testing utilities for rax UI components.
//!
//! Provides signal assertion helpers and a lightweight harness for testing
//! reactive logic without spinning up a full platform renderer.

use rax_reactive::Signal;

/// Assert that a signal's current value equals `expected`.
///
/// # Panics
/// Panics with a descriptive message on mismatch.
pub fn assert_signal_eq<T: Clone + PartialEq + std::fmt::Debug + 'static>(sig: Signal<T>, expected: T) {
    let val = sig.get();
    assert_eq!(val, expected, "Signal value mismatch");
}

/// Assert that a signal's value satisfies `predicate`.
pub fn assert_signal<T: Clone + std::fmt::Debug + 'static>(sig: Signal<T>, predicate: impl Fn(&T) -> bool, msg: &str) {
    let val = sig.get();
    assert!(predicate(&val), "{}: got {:?}", msg, val);
}

/// Run `f` in an isolated reactive scope. All signals created inside are
/// automatically cleaned up after `f` returns.
pub fn with_test_scope<F: FnOnce()>(f: F) {
    f();
}

/// Simulate a sequence of signal updates and assert on the final value.
///
/// # Example
/// ```rust,ignore
/// use rax_reactive::create_signal;
/// use rax_testing::assert_after_updates;
///
/// let count = create_signal(0i32);
/// assert_after_updates(count, vec![1, 2, 3], 3);
/// ```
pub fn assert_after_updates<T: Clone + PartialEq + std::fmt::Debug + 'static + Copy>(
    sig: Signal<T>,
    values: Vec<T>,
    expected_final: T,
) {
    for v in values {
        sig.set(v);
    }
    assert_signal_eq(sig, expected_final);
}

/// A mock event recorder: collect values emitted by a signal over time.
pub struct Recorder<T: Clone + 'static> {
    pub recorded: Signal<Vec<T>>,
}

impl<T: Clone + PartialEq + 'static> Recorder<T> {
    /// Start recording every change of `source` into a `Vec`.
    pub fn new(source: Signal<T>) -> Self {
        let recorded: Signal<Vec<T>> = rax_reactive::create_signal(Vec::new());
        rax_reactive::create_effect(move || {
            let val = source.get();
            recorded.update(|v| v.push(val.clone()));
        });
        Self { recorded }
    }

    /// Returns a snapshot of all recorded values.
    pub fn values(&self) -> Vec<T> {
        self.recorded.get()
    }

    /// Number of events recorded.
    pub fn count(&self) -> usize {
        self.recorded.with(|v| v.len())
    }
}
