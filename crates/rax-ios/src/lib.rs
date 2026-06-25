//! iOS (UIKit) backend for `rax`, written in pure Rust via `objc2`.
//!
//! There is no Swift or Objective-C source: the app delegate, views, button
//! target-action, and the `CADisplayLink` frame driver are all defined in Rust.
//! [`run`] is the single entry point an app's `main` calls.
//!
//! The backend translates the engine's [`Mutation`](rax_dom::Mutation) stream
//! into `UIView`/`UILabel`/`UIButton` operations, positions views from the
//! layout pass's frames, and forwards taps back through the runtime's event
//! sink. A per-thread `STATE` holds the running app so the objc callbacks
//! (tap, frame tick) can reach it without storing Rust state in ivars.

#![doc(html_no_source)]

#[cfg(target_os = "ios")]
mod ios;

#[cfg(target_os = "ios")]
pub use ios::run;

/// Entry point: hands control to UIKit and mounts the view produced by
/// `make_view`. Never returns. On non-iOS targets this panics (the crate exists
/// so the workspace builds everywhere).
#[cfg(not(target_os = "ios"))]
pub fn run<V, F>(_make_view: F) -> !
where
    F: FnOnce() -> V + 'static,
    V: rax_view::View,
{
    panic!("rax-ios::run is only available on iOS targets");
}
