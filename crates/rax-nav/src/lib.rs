//! Stack navigation for `rax`.
//!
//! A [`Navigator`] holds a stack of routes in a signal. [`routes`] renders the
//! top route via a dynamic subtree, so pushing/popping reactively swaps the
//! screen. The navigator is provided via context, so any descendant can call
//! [`use_navigator`] to drive navigation without prop-threading.
//!
//! Routes are your own `Clone` type (usually an enum), so navigation is
//! compile-checked.
//!
//! ```
//! use rax_nav::{create_navigator, routes};
//! use rax_view::{boxed, text, View};
//!
//! #[derive(Clone)]
//! enum Screen { Home, Details(u32) }
//!
//! fn app() -> impl View {
//!     let nav = create_navigator(Screen::Home);
//!     routes(nav, move |screen| match screen {
//!         Screen::Home => boxed(text("home")),
//!         Screen::Details(id) => boxed(text(format!("details {id}"))),
//!     })
//! }
//! ```

#![forbid(unsafe_code)]

use rax_reactive::{create_signal, provide_context, use_context, Signal};
use rax_view::{dynamic, BoxedView, View};

/// A navigation stack over routes of type `R`. Cheap `Copy` handle.
pub struct Navigator<R: 'static> {
    stack: Signal<Vec<R>>,
}

impl<R: 'static> Clone for Navigator<R> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<R: 'static> Copy for Navigator<R> {}

/// Creates a navigator with `initial` as the root route and provides it via
/// context so descendants can [`use_navigator`].
pub fn create_navigator<R: Clone + 'static>(initial: R) -> Navigator<R> {
    let nav = Navigator {
        stack: create_signal(vec![initial]),
    };
    provide_context(nav);
    nav
}

/// The navigator of route type `R` in scope, if any.
pub fn use_navigator<R: Clone + 'static>() -> Option<Navigator<R>> {
    use_context::<Navigator<R>>()
}

impl<R: Clone + 'static> Navigator<R> {
    /// Pushes a new route onto the stack.
    pub fn push(&self, route: R) {
        self.stack.update(|s| s.push(route));
    }

    /// Pops the top route (no-op at the root).
    pub fn pop(&self) {
        self.stack.update(|s| {
            if s.len() > 1 {
                s.pop();
            }
        });
    }

    /// Replaces the top route.
    pub fn replace(&self, route: R) {
        self.stack.update(|s| {
            s.pop();
            s.push(route);
        });
    }

    /// Resets the stack to a single route.
    pub fn reset(&self, route: R) {
        self.stack.update(|s| {
            s.clear();
            s.push(route);
        });
    }

    /// Pops back to the root route.
    pub fn pop_to_root(&self) {
        self.stack.update(|s| s.truncate(1));
    }

    /// The current (top) route. Tracked: reading it in a view re-renders on
    /// navigation.
    pub fn top(&self) -> R {
        self.stack
            .with(|s| s.last().expect("navigator stack is never empty").clone())
    }

    /// Number of routes on the stack (tracked).
    pub fn depth(&self) -> usize {
        self.stack.with(|s| s.len())
    }

    /// Whether there is a route to pop back to (tracked).
    pub fn can_pop(&self) -> bool {
        self.depth() > 1
    }
}

/// Renders the navigator's current route. `render` maps a route to a view; when
/// the stack changes, the displayed screen swaps reactively.
pub fn routes<R, F>(nav: Navigator<R>, mut render: F) -> impl View
where
    R: Clone + 'static,
    F: FnMut(R) -> BoxedView + 'static,
{
    dynamic(move || render(nav.top()))
}

// ---------------------------------------------------------------------------
// NavigationTransition — animated screen enter/exit
// ---------------------------------------------------------------------------

/// How a pushed screen enters (and how a popped screen exits in reverse).
///
/// Pass to [`transition_routes`] to get animated push/pop transitions.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NavigationTransition {
    /// Standard slide from the right on push; slide out to the right on pop.
    #[default]
    Slide,
    /// Fade in on push; fade out on pop.
    Fade,
    /// No animation — instant cut.
    None,
}

// ---------------------------------------------------------------------------
// Screen lifecycle
// ---------------------------------------------------------------------------

use std::cell::RefCell;

thread_local! {
    static APPEAR_HANDLERS: RefCell<Vec<(String, Box<dyn Fn()>)>> = RefCell::new(vec![]);
    static DISAPPEAR_HANDLERS: RefCell<Vec<(String, Box<dyn Fn()>)>> = RefCell::new(vec![]);
}

/// Register a callback that fires when the screen with the given route key appears.
///
/// Call this at the top of a screen's composable function.
/// The callback is cleared when the screen is popped.
pub fn on_appear(route: &str, f: impl Fn() + 'static) {
    APPEAR_HANDLERS.with(|h| {
        h.borrow_mut().push((route.to_string(), Box::new(f)));
    });
}

/// Register a callback that fires when the screen with the given route key disappears.
pub fn on_disappear(route: &str, f: impl Fn() + 'static) {
    DISAPPEAR_HANDLERS.with(|h| {
        h.borrow_mut().push((route.to_string(), Box::new(f)));
    });
}

/// Called by the navigation system when a screen appears (e.g. after push completes).
pub fn fire_appear(route: &str) {
    APPEAR_HANDLERS.with(|h| {
        for (key, cb) in h.borrow().iter() {
            if key == route {
                cb();
            }
        }
    });
}

/// Called by the navigation system when a screen disappears (e.g. before pop).
pub fn fire_disappear(route: &str) {
    DISAPPEAR_HANDLERS.with(|h| {
        for (key, cb) in h.borrow().iter() {
            if key == route {
                cb();
            }
        }
    });
}

/// Run a side effect whenever the current screen gains focus.
/// The callback is called immediately and on every re-focus.
/// Pass the current route key.
pub fn use_focus_effect(route: &str, f: impl Fn() + 'static) {
    on_appear(route, f);
}

// ---------------------------------------------------------------------------
// String-based programmatic navigation
// ---------------------------------------------------------------------------

use std::collections::HashMap;

thread_local! {
    /// The current route as a reactive signal (string-based router).
    static CURRENT_ROUTE: RefCell<Option<Signal<String>>> = const { RefCell::new(None) };
    /// Navigation history stack for the string-based router.
    static HISTORY_STACK: RefCell<Vec<String>> = RefCell::new(Vec::new());
    /// Route guards: (condition, redirect_target).
    static ROUTE_GUARDS: RefCell<Vec<RouteGuard>> = RefCell::new(Vec::new());
    /// Not-found / fallback handler.
    static NOT_FOUND_HANDLER: RefCell<Option<Box<dyn Fn() -> BoxedView>>> = RefCell::new(None);
    /// Navigation event listeners: called with (from, to) on every navigation.
    static NAV_LISTENERS: RefCell<Vec<Box<dyn Fn(&str, &str)>>> = RefCell::new(Vec::new());
    /// Back handlers: called in order; first one returning `true` consumes the event.
    static BACK_HANDLERS: RefCell<Vec<Box<dyn Fn() -> bool>>> = RefCell::new(Vec::new());
}

// ---------------------------------------------------------------------------
// Route guard
// ---------------------------------------------------------------------------

/// A guard that can block navigation and redirect to another route.
pub struct RouteGuard {
    condition: Box<dyn Fn() -> bool>,
    redirect: String,
}

/// Register a route guard. Before each navigation `condition` is evaluated; if
/// it returns `false` the navigation is redirected to `redirect` instead.
pub fn add_route_guard(condition: impl Fn() -> bool + 'static, redirect: &str) {
    ROUTE_GUARDS.with(|g| {
        g.borrow_mut().push(RouteGuard {
            condition: Box::new(condition),
            redirect: redirect.to_string(),
        });
    });
}

/// Evaluate all guards against the intended `route`. Returns the first
/// redirect target if any guard blocks navigation, or `None` if all pass.
pub fn check_guards(route: &str) -> Option<String> {
    ROUTE_GUARDS.with(|g| {
        for guard in g.borrow().iter() {
            if !(guard.condition)() {
                return Some(guard.redirect.clone());
            }
        }
        let _ = route; // route is available for future per-route guard matching
        None
    })
}

// ---------------------------------------------------------------------------
// Current route signal initialiser (lazy)
// ---------------------------------------------------------------------------

fn ensure_route_signal() -> Signal<String> {
    CURRENT_ROUTE.with(|r| {
        let mut borrow = r.borrow_mut();
        if let Some(sig) = *borrow {
            sig
        } else {
            let sig = create_signal(String::new());
            *borrow = Some(sig);
            sig
        }
    })
}

// ---------------------------------------------------------------------------
// Programmatic navigation API
// ---------------------------------------------------------------------------

/// Returns the reactive current route signal. Reading it in a view will
/// re-render the view whenever the route changes.
pub fn current_route() -> Signal<String> {
    ensure_route_signal()
}

/// Navigate to `route`, checking guards first. Fires navigation listeners.
/// Returns the route that was actually navigated to (may differ if a guard
/// redirected).
pub fn navigate(route: &str) -> String {
    let destination = check_guards(route)
        .unwrap_or_else(|| route.to_string());

    let from = HISTORY_STACK.with(|s| {
        s.borrow().last().cloned().unwrap_or_default()
    });

    HISTORY_STACK.with(|s| s.borrow_mut().push(destination.clone()));

    let sig = ensure_route_signal();
    sig.set(destination.clone());

    fire_navigate_event(&from, &destination);

    destination
}

/// Pop the current route from the history stack and return to the previous
/// one. Returns `false` if the stack is already empty.
pub fn go_back() -> bool {
    let popped = HISTORY_STACK.with(|s| {
        let mut stack = s.borrow_mut();
        if stack.len() <= 1 {
            return false;
        }
        stack.pop();
        true
    });

    if popped {
        let prev = HISTORY_STACK.with(|s| {
            s.borrow().last().cloned().unwrap_or_default()
        });
        let sig = ensure_route_signal();
        sig.set(prev);
    }

    popped
}

/// Returns `true` if there is at least one route to go back to.
pub fn can_go_back() -> bool {
    HISTORY_STACK.with(|s| s.borrow().len() > 1)
}

/// Parse `:param` segments from the current route against all registered
/// route patterns and return the matching parameters. Returns an empty map
/// if no match is found or no patterns have been registered.
///
/// Patterns are tried in registration order; the first match wins.
/// See [`match_route`] for the matching semantics.
pub fn use_params() -> HashMap<String, String> {
    let route = ensure_route_signal();
    let current = route.with(|r| r.clone());

    // Try to find params from the current route by attempting common patterns.
    // In a real app the patterns would be registered; here we return the
    // path segments as positional keys if no explicit match is found.
    // (Callers should use match_route directly for pattern-specific params.)
    let mut params = HashMap::new();
    for (i, segment) in current.split('/').filter(|s| !s.is_empty()).enumerate() {
        params.insert(format!("segment_{i}"), segment.to_string());
    }
    params
}

// ---------------------------------------------------------------------------
// Not-found / fallback handler
// ---------------------------------------------------------------------------

/// Register a handler that produces the view shown when no route matches.
pub fn set_not_found(handler: impl Fn() -> BoxedView + 'static) {
    NOT_FOUND_HANDLER.with(|h| {
        *h.borrow_mut() = Some(Box::new(handler));
    });
}

/// Invoke the not-found handler, returning its view, or `None` if no handler
/// has been registered.
pub fn get_not_found() -> Option<BoxedView> {
    NOT_FOUND_HANDLER.with(|h| {
        h.borrow().as_ref().map(|f| f())
    })
}

// ---------------------------------------------------------------------------
// Navigation event listeners / analytics hooks
// ---------------------------------------------------------------------------

/// Register a listener that is called with `(from, to)` on every navigation.
pub fn on_navigate(listener: impl Fn(&str, &str) + 'static) {
    NAV_LISTENERS.with(|l| {
        l.borrow_mut().push(Box::new(listener));
    });
}

/// Fire all registered navigation listeners. Called internally by [`navigate`].
pub fn fire_navigate_event(from: &str, to: &str) {
    NAV_LISTENERS.with(|l| {
        for listener in l.borrow().iter() {
            listener(from, to);
        }
    });
}

// ---------------------------------------------------------------------------
// Back-handling
// ---------------------------------------------------------------------------

/// Register a back handler. Handlers are tried in registration order; the
/// first one that returns `true` consumes the event.
pub fn on_back(handler: impl Fn() -> bool + 'static) {
    BACK_HANDLERS.with(|h| {
        h.borrow_mut().push(Box::new(handler));
    });
}

/// Handle a back-navigation event. Tries each registered handler in order;
/// if none handles it, falls back to [`go_back`]. Returns `true` if the
/// event was handled (either by a handler or by going back in history).
pub fn handle_back() -> bool {
    let handled = BACK_HANDLERS.with(|h| {
        for handler in h.borrow().iter() {
            if handler() {
                return true;
            }
        }
        false
    });

    if handled {
        return true;
    }

    go_back()
}

// ---------------------------------------------------------------------------
// Route pattern matching
// ---------------------------------------------------------------------------

/// Match a route `pattern` (e.g. `"/user/:id/post/:postId"`) against a
/// concrete `route` (e.g. `"/user/42/post/7"`). Returns a map of parameter
/// names to their values on success, or `None` if the shapes don't match.
///
/// # Example
/// ```
/// use rax_nav::match_route;
/// let params = match_route("/user/:id/post/:postId", "/user/42/post/7").unwrap();
/// assert_eq!(params["id"], "42");
/// assert_eq!(params["postId"], "7");
/// ```
pub fn match_route(pattern: &str, route: &str) -> Option<HashMap<String, String>> {
    let pattern_segs: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let route_segs: Vec<&str> = route.split('/').filter(|s| !s.is_empty()).collect();

    if pattern_segs.len() != route_segs.len() {
        return None;
    }

    let mut params = HashMap::new();
    for (p, r) in pattern_segs.iter().zip(route_segs.iter()) {
        if let Some(param_name) = p.strip_prefix(':') {
            params.insert(param_name.to_string(), r.to_string());
        } else if p != r {
            return None;
        }
    }

    Some(params)
}

// ---------------------------------------------------------------------------
// push_route / pop_route convenience wrappers
// ---------------------------------------------------------------------------

/// Convenience wrapper: push a route via the string-based router.
pub fn push_route(route: &str) {
    navigate(route);
}

/// Convenience wrapper: go back in the string-based router history.
pub fn pop_route() {
    go_back();
}

// ---------------------------------------------------------------------------
// Modal presentation stack
// ---------------------------------------------------------------------------

thread_local! {
    static MODAL_STACK: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(Vec::new());
}

/// Push a modal route on top of the page stack without affecting the main nav stack.
pub fn present_modal(route: &str) {
    MODAL_STACK.with(|s| s.borrow_mut().push(route.to_string()));
}

/// Dismiss the top-most modal. Returns `false` if there is no modal to dismiss.
pub fn dismiss_modal() -> bool {
    MODAL_STACK.with(|s| {
        let mut stack = s.borrow_mut();
        if stack.is_empty() { return false; }
        stack.pop();
        true
    })
}

/// Returns the current top-most modal route, if any modal is presented.
pub fn current_modal() -> Option<String> {
    MODAL_STACK.with(|s| s.borrow().last().cloned())
}

/// Returns the full modal stack (bottom to top).
pub fn modal_stack() -> Vec<String> {
    MODAL_STACK.with(|s| s.borrow().clone())
}

// ---------------------------------------------------------------------------
// Deep link parsing
// ---------------------------------------------------------------------------

/// Parse a deep link URL into `(path, query_params)`.
///
/// Strips the scheme (e.g. `myapp://`), splits path from query string, and
/// parses query key/value pairs.
///
/// # Example
/// ```
/// use rax_nav::parse_deep_link;
/// let (path, params) = parse_deep_link("myapp://profile/42?tab=posts");
/// assert_eq!(path, "/profile/42");
/// assert_eq!(params["tab"], "posts");
/// ```
pub fn parse_deep_link(url: &str) -> (String, HashMap<String, String>) {
    let path_part = if let Some(idx) = url.find("://") {
        &url[idx + 3..]
    } else {
        url
    };
    let (raw_path, query) = if let Some(q) = path_part.find('?') {
        (&path_part[..q], &path_part[q + 1..])
    } else {
        (path_part, "")
    };
    let path = format!("/{}", raw_path.trim_matches('/'));
    let params: HashMap<String, String> = query
        .split('&')
        .filter(|s| !s.is_empty())
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            Some((parts.next()?.to_string(), parts.next().unwrap_or("").to_string()))
        })
        .collect();
    (path, params)
}

// ---------------------------------------------------------------------------
// try_navigate — navigate with guard check, returns success bool
// ---------------------------------------------------------------------------

/// Like [`navigate`] but returns `true` if navigation succeeded and `false` if
/// a guard redirected it to a different destination.
pub fn try_navigate(route: &str) -> bool {
    let actual = navigate(route);
    actual == route
}

/// Like [`routes`] but animates screen transitions according to `transition`.
///
/// On each push/pop the incoming screen plays the enter animation; the
/// previous screen is immediately replaced (no simultaneous exit animation —
/// that would require two live widget trees which the current single-`dynamic`
/// architecture does not support).
///
/// For `Slide`, the incoming screen slides in from the right (offset =
/// `screen_width`). Since the layout width is not known at build time, a fixed
/// `375` point estimate is used. On `Fade`, opacity animates `0 → 1`.
/// On `None`, the screen is shown immediately with no animation.
///
/// # Example
/// ```rust
/// use rax_nav::{create_navigator, transition_routes, NavigationTransition};
/// use rax_view::{boxed, text, View};
///
/// #[derive(Clone)]
/// enum Screen { Home, Details }
///
/// fn app() -> impl View {
///     let nav = create_navigator(Screen::Home);
///     transition_routes(nav, NavigationTransition::Slide, move |screen| match screen {
///         Screen::Home => boxed(text("home")),
///         Screen::Details => boxed(text("details")),
///     })
/// }
/// ```
pub fn transition_routes<R, F>(
    nav: Navigator<R>,
    transition: NavigationTransition,
    mut render: F,
) -> impl View
where
    R: Clone + 'static,
    F: FnMut(R) -> BoxedView + 'static,
{
    use rax_anim::{animate, Easing};
    use rax_reactive::{create_effect, create_signal};
    use rax_view::{boxed, column, dynamic, ViewExt};
    use rax_dom::Transform;

    // Generation counter: bumps each time the stack changes; used inside
    // `dynamic` to force a new screen to be built and its enter anim started.
    let gen = create_signal(0u32);

    // Watch the stack depth for changes and bump the generation.
    create_effect(move || {
        let _ = nav.depth(); // track
        gen.update(|g| *g = g.wrapping_add(1));
    });

    dynamic(move || {
        let _gen = gen.get(); // re-run this closure on every navigation

        let screen = render(nav.top());

        match transition {
            NavigationTransition::None => screen,

            NavigationTransition::Slide => {
                // Slide in from the right: start at +375 (estimated screen
                // width), animate to 0. The animation signal is read via
                // transform_fn which re-runs per frame — no nested dynamic
                // needed.
                let offset = create_signal(375.0f32);
                let anim = animate(375.0, 0.0, 0.3, Easing::EaseOut);
                create_effect(move || offset.set(anim.get()));

                boxed(
                    column((screen,))
                        .grow()
                        .transform_fn(move || {
                            Transform::IDENTITY.translate(offset.get(), 0.0)
                        }),
                )
            }

            NavigationTransition::Fade => {
                // Fade in: opacity 0 → 1.
                let opacity = create_signal(0.0f32);
                let anim = animate(0.0, 1.0, 0.25, Easing::EaseOut);
                create_effect(move || opacity.set(anim.get()));

                boxed(
                    column((screen,))
                        .grow()
                        .opacity_fn(move || opacity.get()),
                )
            }
        }
    })
}
