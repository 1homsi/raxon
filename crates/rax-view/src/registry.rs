//! Component registry: replace any built-in widget app-wide.
//!
//! Register a factory function keyed by component name. When a composable
//! calls [`resolve_component`], it checks the registry first and falls back
//! to the built-in if nothing is registered.
//!
//! The registry is **thread-local**: each OS thread (UI thread, test thread,
//! …) has its own independent map. This avoids locking overhead and matches the
//! single-threaded assumption of the rest of rax.
//!
//! # Example
//! ```no_run
//! use rax_view::{register_component, ComponentProps};
//! use rax_view::text;
//! use rax_view::modifier::ViewExt;
//!
//! // Register a custom button factory at startup (before any widgets build):
//! register_component("Button", |props: &ComponentProps| {
//!     rax_view::boxed(text(props.label.clone()).padding(8.0))
//! });
//! ```

use std::collections::HashMap;
use std::cell::RefCell;

use crate::view::BoxedView;

// ---------------------------------------------------------------------------
// ComponentProps
// ---------------------------------------------------------------------------

/// Arbitrary string properties forwarded to a component factory.
///
/// Built with a fluent builder:
/// ```no_run
/// use rax_view::ComponentProps;
/// let props = ComponentProps::new()
///     .label("Confirm")
///     .variant("primary")
///     .set("icon", "check");
/// ```
#[derive(Debug, Clone, Default)]
pub struct ComponentProps {
    /// Primary text label (e.g. button title, chip label).
    pub label: String,
    /// Style variant hint (e.g. `"primary"`, `"ghost"`, `"destructive"`).
    pub variant: String,
    /// Arbitrary key/value extensions for factory-specific options.
    pub extra: HashMap<String, String>,
}

impl ComponentProps {
    /// Create an empty props bundle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the `label` field.
    pub fn label(mut self, s: impl Into<String>) -> Self {
        self.label = s.into();
        self
    }

    /// Set the `variant` field.
    pub fn variant(mut self, s: impl Into<String>) -> Self {
        self.variant = s.into();
        self
    }

    /// Insert an arbitrary key/value pair into `extra`.
    pub fn set(mut self, key: &str, val: &str) -> Self {
        self.extra.insert(key.into(), val.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Registry internals
// ---------------------------------------------------------------------------

type Factory = Box<dyn Fn(&ComponentProps) -> BoxedView>;

thread_local! {
    static REGISTRY: RefCell<HashMap<String, Factory>> = RefCell::new(HashMap::new());
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Register a custom factory for `name`.
///
/// The factory receives a [`ComponentProps`] bundle and must return a
/// [`BoxedView`]. Any previously registered factory for the same name is
/// silently replaced.
///
/// Call this **once at startup**, before any widgets that reference the
/// component are built.
///
/// # Example
/// ```no_run
/// register_component("Button", |props| {
///     rax_view::boxed(rax_view::text(props.label.clone()))
/// });
/// ```
pub fn register_component(
    name: &str,
    factory: impl Fn(&ComponentProps) -> BoxedView + 'static,
) {
    REGISTRY.with(|r| {
        r.borrow_mut().insert(name.to_string(), Box::new(factory));
    });
}

/// Remove the factory registered under `name`, reverting to the built-in.
///
/// A no-op if nothing was registered.
pub fn unregister_component(name: &str) {
    REGISTRY.with(|r| {
        r.borrow_mut().remove(name);
    });
}

/// Returns `true` if a custom factory has been registered for `name`.
pub fn is_registered(name: &str) -> bool {
    REGISTRY.with(|r| r.borrow().contains_key(name))
}

/// Invoke the registered factory for `name`, if any.
///
/// Returns `Some(BoxedView)` when a custom factory is found, `None` when the
/// name is unknown — callers should then build the built-in component instead.
///
/// # Example
/// ```no_run
/// use rax_view::{resolve_component, ComponentProps, button};
///
/// fn my_button(label: &str, on_press: impl Fn() + 'static) -> impl rax_view::View {
///     let props = ComponentProps::new().label(label);
///     if let Some(custom) = resolve_component("Button", &props) {
///         return custom;
///     }
///     rax_view::boxed(button(label, on_press))
/// }
/// ```
pub fn resolve_component(name: &str, props: &ComponentProps) -> Option<BoxedView> {
    REGISTRY.with(|r| {
        r.borrow().get(name).map(|f| f(props))
    })
}
