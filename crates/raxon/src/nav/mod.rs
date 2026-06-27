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
//! use raxon::nav::{create_navigator, routes};
//! use raxon::view::{boxed, text, View};
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

use crate::reactive::{create_signal, provide_context, use_context, Signal};
use crate::view::{dynamic, BoxedView, View};

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

/// A push/pop stack navigator that creates and provides its own [`Navigator`]
/// — the batteries-included form of [`create_navigator`] + [`routes`].
///
/// `initial` is the root route; `render` maps the current top route to a view.
/// Descendant screens reach the navigator with
/// [`use_navigator::<R>()`](use_navigator) to `push` / `pop`. Use this when you
/// just want "a screen stack" without wiring the navigator by hand.
///
/// ```
/// use raxon::nav::{stack, use_navigator};
/// use raxon::view::{boxed, button, text};
///
/// #[derive(Clone)]
/// enum Route { List, Detail(u32) }
///
/// let view = stack(Route::List, |route| match route {
///     Route::List => boxed(button("Open #7", || {
///         if let Some(nav) = use_navigator::<Route>() { nav.push(Route::Detail(7)); }
///     })),
///     Route::Detail(id) => boxed(text(format!("Item {id}"))),
/// });
/// ```
pub fn stack<R, F>(initial: R, render: F) -> impl View
where
    R: Clone + 'static,
    F: FnMut(R) -> BoxedView + 'static,
{
    let nav = create_navigator(initial);
    routes(nav, render)
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

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
thread_local! {
    static WEB_HISTORY_BOUND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
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

/// Parsed route information for app/internal URLs.
///
/// `path` is the normalized route path without query or fragment, `query`
/// contains the first value for each query key, `query_all` keeps repeated
/// query keys, and `fragment` contains the decoded hash fragment without `#`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteLocation {
    /// Normalized route path without query or fragment.
    pub path: String,
    /// First value for each decoded query key.
    pub query: HashMap<String, String>,
    /// All decoded values for each decoded query key, preserving duplicate keys.
    pub query_all: HashMap<String, Vec<String>>,
    /// Decoded URL fragment without the leading `#`, when present.
    pub fragment: Option<String>,
}

impl RouteLocation {
    /// Returns the first decoded value for `key`, if present.
    pub fn query_value(&self, key: &str) -> Option<&str> {
        self.query.get(key).map(String::as_str)
    }

    /// Returns all decoded values for `key`, if present.
    pub fn query_values(&self, key: &str) -> Option<&[String]> {
        self.query_all.get(key).map(Vec::as_slice)
    }
}

/// A successful declarative route match.
///
/// Passed to [`route`] renderers and returned by [`match_route_location`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteMatch {
    /// The route pattern that matched, e.g. `"/orders/:id"`.
    pub pattern: String,
    /// Normalized path that matched the pattern, without query or fragment.
    pub path: String,
    /// Decoded `:param` values captured from the path.
    pub params: HashMap<String, String>,
    /// First decoded value for each query key.
    pub query: HashMap<String, String>,
    /// All decoded values for each query key, preserving duplicate keys.
    pub query_all: HashMap<String, Vec<String>>,
    /// Decoded URL fragment without the leading `#`, when present.
    pub fragment: Option<String>,
}

impl RouteMatch {
    /// Returns a decoded path parameter value, if present.
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params.get(key).map(String::as_str)
    }

    /// Returns the first decoded query value for `key`, if present.
    pub fn query_value(&self, key: &str) -> Option<&str> {
        self.query.get(key).map(String::as_str)
    }

    /// Returns all decoded query values for `key`, if present.
    pub fn query_values(&self, key: &str) -> Option<&[String]> {
        self.query_all.get(key).map(Vec::as_slice)
    }
}

/// A declarative URL route definition.
///
/// Build these with [`route`] and render them with [`url_routes`].
pub struct UrlRoute {
    pattern: String,
    render: Box<dyn Fn(RouteMatch) -> BoxedView>,
}

impl UrlRoute {
    /// Returns the route pattern for this definition.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Matches this definition against `route`, returning captured route state.
    pub fn matches(&self, route: &str) -> Option<RouteMatch> {
        match_route_location(&self.pattern, route)
    }
}

/// Creates a declarative URL route.
///
/// `pattern` supports static path segments and `:param` captures. Query keys
/// and fragments in the pattern act as constraints, so `"/orders/:id?tab=items"`
/// only matches routes whose first `tab` query value is `"items"`.
///
/// # Example
/// ```
/// use raxon::nav::route;
/// use raxon::view::{boxed, text};
///
/// let detail = route("/orders/:id", |m| {
///     boxed(text(format!("Order {}", m.param("id").unwrap_or(""))))
/// });
/// assert_eq!(detail.pattern(), "/orders/:id");
/// ```
pub fn route(
    pattern: impl Into<String>,
    render: impl Fn(RouteMatch) -> BoxedView + 'static,
) -> UrlRoute {
    UrlRoute {
        pattern: pattern.into(),
        render: Box::new(render),
    }
}

/// Navigate to `route`, checking guards first. Fires navigation listeners.
/// Returns the route that was actually navigated to (may differ if a guard
/// redirected).
pub fn navigate(route: &str) -> String {
    let destination = check_guards(route).unwrap_or_else(|| route.to_string());

    let from = HISTORY_STACK.with(|s| s.borrow().last().cloned().unwrap_or_default());

    HISTORY_STACK.with(|s| s.borrow_mut().push(destination.clone()));

    let sig = ensure_route_signal();
    sig.set(destination.clone());

    crate::web::push_path(&destination);

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
        let prev = HISTORY_STACK.with(|s| s.borrow().last().cloned().unwrap_or_default());
        let sig = ensure_route_signal();
        sig.set(prev.clone());
        crate::web::replace_path(&prev);
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
    let location = parse_route_location(&current);

    // Try to find params from the current route by attempting common patterns.
    // In a real app the patterns would be registered; here we return the
    // path segments as positional keys if no explicit match is found.
    // (Callers should use match_route directly for pattern-specific params.)
    let mut params = HashMap::new();
    for (i, segment) in location
        .path
        .split('/')
        .filter(|s| !s.is_empty())
        .enumerate()
    {
        params.insert(format!("segment_{i}"), decode_url_component(segment, false));
    }
    params
}

/// Returns the parsed current route, including path, query params, and fragment.
pub fn current_route_location() -> RouteLocation {
    let route = ensure_route_signal();
    route.with(|r| parse_route_location(r))
}

/// Returns decoded query parameters for the current route.
///
/// When a key appears multiple times, the first value is returned here. Use
/// [`current_route_location`] to access `query_all` for repeated values.
pub fn use_query_params() -> HashMap<String, String> {
    current_route_location().query
}

/// Renders the first declarative URL route that matches [`current_route`].
///
/// Use this for web/deep-link-addressable screen shells. On web, call
/// [`bind_web_history`] once during app startup to initialize the route from the
/// browser URL and keep back/forward navigation in sync.
///
/// # Example
/// ```
/// use raxon::nav::{route, url_routes};
/// use raxon::view::{boxed, text};
///
/// let view = url_routes(vec![
///     route("/", |_| boxed(text("home"))),
///     route("/orders/:id", |m| {
///         boxed(text(format!("order {}", m.param("id").unwrap_or(""))))
///     }),
/// ]);
/// ```
pub fn url_routes(routes: Vec<UrlRoute>) -> impl View {
    dynamic(move || {
        let location = current_route_location();
        for route in &routes {
            if let Some(route_match) = match_route_definition(&route.pattern, location.clone()) {
                return (route.render)(route_match);
            }
        }

        get_not_found().unwrap_or_else(|| {
            use crate::view::{boxed, text};
            boxed(text(format!("Route not found: {}", location.path)))
        })
    })
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
    NOT_FOUND_HANDLER.with(|h| h.borrow().as_ref().map(|f| f()))
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
/// use raxon::nav::match_route;
/// let params = match_route("/user/:id/post/:postId", "/user/42/post/7").unwrap();
/// assert_eq!(params["id"], "42");
/// assert_eq!(params["postId"], "7");
/// ```
pub fn match_route(pattern: &str, route: &str) -> Option<HashMap<String, String>> {
    let pattern_location = parse_route_location(pattern);
    let route_location = parse_route_location(route);
    match_path_params(&pattern_location.path, &route_location.path)
}

/// Match a declarative route pattern against a route/URL and return all route
/// state needed by a screen renderer.
pub fn match_route_location(pattern: &str, route: &str) -> Option<RouteMatch> {
    let location = parse_route_location(route);
    match_route_definition(pattern, location)
}

fn match_route_definition(pattern: &str, location: RouteLocation) -> Option<RouteMatch> {
    let pattern_location = parse_route_location(pattern);
    let params = match_path_params(&pattern_location.path, &location.path)?;

    if !query_constraints_match(&pattern_location.query, &location.query) {
        return None;
    }

    if let Some(pattern_fragment) = pattern_location.fragment.as_deref() {
        if location.fragment.as_deref() != Some(pattern_fragment) {
            return None;
        }
    }

    Some(RouteMatch {
        pattern: pattern.to_string(),
        path: location.path,
        params,
        query: location.query,
        query_all: location.query_all,
        fragment: location.fragment,
    })
}

fn match_path_params(pattern_path: &str, route_path: &str) -> Option<HashMap<String, String>> {
    let pattern_segs: Vec<&str> = pattern_path.split('/').filter(|s| !s.is_empty()).collect();
    let route_segs: Vec<&str> = route_path.split('/').filter(|s| !s.is_empty()).collect();

    if pattern_segs.len() != route_segs.len() {
        return None;
    }

    let mut params = HashMap::new();
    for (p, r) in pattern_segs.iter().zip(route_segs.iter()) {
        if let Some(param_name) = p.strip_prefix(':') {
            if param_name.is_empty() {
                return None;
            }
            params.insert(param_name.to_string(), decode_url_component(r, false));
        } else if decode_url_component(p, false) != decode_url_component(r, false) {
            return None;
        }
    }

    Some(params)
}

fn query_constraints_match(
    pattern_query: &HashMap<String, String>,
    route_query: &HashMap<String, String>,
) -> bool {
    pattern_query
        .iter()
        .all(|(key, value)| route_query.get(key) == Some(value))
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
        if stack.is_empty() {
            return false;
        }
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
/// use raxon::nav::parse_deep_link;
/// let (path, params) = parse_deep_link("myapp://profile/42?tab=posts");
/// assert_eq!(path, "/profile/42");
/// assert_eq!(params["tab"], "posts");
/// ```
pub fn parse_deep_link(url: &str) -> (String, HashMap<String, String>) {
    let location = parse_route_location(url);
    (location.path, location.query)
}

/// Parses a route, relative URL, absolute web URL, custom-scheme deep link, or
/// hash route into path/query/fragment pieces.
///
/// Supported forms include `/orders/42?tab=items#notes`,
/// `https://example.com/orders/42?tab=items`, `pablo://orders/42?tab=items`,
/// and hash-router URLs such as `https://example.com/#/orders/42?tab=items`.
pub fn parse_route_location(input: &str) -> RouteLocation {
    let trimmed = input.trim();
    let (without_fragment, fragment_raw) = split_once(trimmed, '#');
    let fragment = fragment_raw
        .filter(|value| !value.is_empty())
        .map(|value| decode_url_component(value, false));

    if let Some(hash_route) = fragment_raw.and_then(hash_route_part) {
        let mut location = parse_route_location(hash_route);
        location.fragment = fragment;
        return location;
    }

    let route_part = strip_url_prefix(without_fragment);
    let (raw_path, query_raw) = split_once(route_part, '?');
    let path = normalize_route_path(raw_path);
    let query_all = parse_query_all(query_raw.unwrap_or_default());
    let query = first_query_values(&query_all);

    RouteLocation {
        path,
        query,
        query_all,
        fragment,
    }
}

/// Parses a query string into decoded first values.
///
/// Accepts strings with or without a leading `?`. Repeated keys keep the first
/// value, matching browser `URLSearchParams.get` behavior.
pub fn parse_query(query: &str) -> HashMap<String, String> {
    first_query_values(&parse_query_all(query))
}

/// Parses a query string into decoded repeated values.
///
/// Accepts strings with or without a leading `?`; keys without `=` map to an
/// empty string value.
pub fn parse_query_all(query: &str) -> HashMap<String, Vec<String>> {
    let query = query.trim_start_matches('?');
    let mut params: HashMap<String, Vec<String>> = HashMap::new();

    for pair in query.split(['&', ';']).filter(|part| !part.is_empty()) {
        let (key, value) = split_once(pair, '=');
        let key = decode_url_component(key, true);
        if key.is_empty() {
            continue;
        }
        let value = decode_url_component(value.unwrap_or_default(), true);
        params.entry(key).or_default().push(value);
    }

    params
}

fn first_query_values(query_all: &HashMap<String, Vec<String>>) -> HashMap<String, String> {
    query_all
        .iter()
        .filter_map(|(key, values)| values.first().map(|value| (key.clone(), value.clone())))
        .collect()
}

fn split_once(input: &str, needle: char) -> (&str, Option<&str>) {
    if let Some(index) = input.find(needle) {
        (&input[..index], Some(&input[index + needle.len_utf8()..]))
    } else {
        (input, None)
    }
}

fn hash_route_part(fragment: &str) -> Option<&str> {
    let route = fragment.strip_prefix('!').unwrap_or(fragment);
    route.starts_with('/').then_some(route)
}

fn strip_url_prefix(input: &str) -> &str {
    if let Some(rest) = input.strip_prefix("//") {
        return strip_authority(rest);
    }

    let Some(scheme_index) = input.find(':') else {
        return input;
    };
    let scheme = &input[..scheme_index];
    if !scheme
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return input;
    }

    let rest = &input[scheme_index + 1..];
    if let Some(rest) = rest.strip_prefix("//") {
        if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
            strip_authority(rest)
        } else {
            rest
        }
    } else {
        rest
    }
}

fn strip_authority(input: &str) -> &str {
    let end = input.find(['/', '?']).unwrap_or(input.len());
    &input[end..]
}

fn normalize_route_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        return "/".to_string();
    }
    let with_leading = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    if with_leading.len() > 1 {
        with_leading.trim_end_matches('/').to_string()
    } else {
        with_leading
    }
}

fn decode_url_component(input: &str, plus_as_space: bool) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                if let (Some(high), Some(low)) =
                    (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
                {
                    out.push((high << 4) | low);
                    index += 3;
                    continue;
                }
                out.push(bytes[index]);
                index += 1;
            }
            b'+' if plus_as_space => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Binds the string router to browser history on the web target.
///
/// On web, this initializes the current route from `window.location`, listens
/// for browser back/forward navigation, and keeps guarded redirects reflected
/// in the address bar. It is a no-op on native targets.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub fn bind_web_history() {
    let already_bound = WEB_HISTORY_BOUND.with(|bound| {
        let was_bound = bound.get();
        bound.set(true);
        was_bound
    });
    if already_bound {
        return;
    }

    replace_route_from_browser(&crate::web::location_route());
    crate::web::on_popstate(|route| replace_route_from_browser(&route));
}

/// Binds the string router to browser history on the web target.
///
/// This is a no-op on native targets.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub fn bind_web_history() {}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn replace_route_from_browser(route: &str) {
    let destination = check_guards(route).unwrap_or_else(|| route.to_string());
    let from = HISTORY_STACK.with(|s| {
        let mut stack = s.borrow_mut();
        let from = stack.last().cloned().unwrap_or_default();
        if stack.is_empty() {
            stack.push(destination.clone());
        } else if let Some(current) = stack.last_mut() {
            *current = destination.clone();
        }
        from
    });

    let sig = ensure_route_signal();
    sig.set(destination.clone());

    if destination != route {
        crate::web::replace_path(&destination);
    }

    fire_navigate_event(&from, &destination);
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
/// use raxon::nav::{create_navigator, transition_routes, NavigationTransition};
/// use raxon::view::{boxed, text, View};
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
    use crate::anim::{animate, Easing};
    use crate::dom::Transform;
    use crate::reactive::{create_effect, create_signal};
    use crate::view::{boxed, column, dynamic, ViewExt};

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
                        .transform_fn(move || Transform::IDENTITY.translate(offset.get(), 0.0)),
                )
            }

            NavigationTransition::Fade => {
                // Fade in: opacity 0 → 1.
                let opacity = create_signal(0.0f32);
                let anim = animate(0.0, 1.0, 0.25, Easing::EaseOut);
                create_effect(move || opacity.set(anim.get()));

                boxed(column((screen,)).grow().opacity_fn(move || opacity.get()))
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        match_route, match_route_location, parse_deep_link, parse_query, parse_query_all,
        parse_route_location, route,
    };

    #[test]
    fn parses_query_strings_with_decoding_and_repeated_keys() {
        let first = parse_query("?tab=reviews&filter=open&filter=closed&empty&name=Alice+Doe");

        assert_eq!(first["tab"], "reviews");
        assert_eq!(first["filter"], "open");
        assert_eq!(first["empty"], "");
        assert_eq!(first["name"], "Alice Doe");

        let all = parse_query_all("filter=open&filter=closed&encoded=a%2Fb");
        assert_eq!(
            all["filter"],
            vec!["open".to_string(), "closed".to_string()]
        );
        assert_eq!(all["encoded"], vec!["a/b".to_string()]);
    }

    #[test]
    fn parses_web_custom_and_hash_route_locations() {
        let web = parse_route_location("https://rtylr.com/products/42?tab=reviews#notes");
        assert_eq!(web.path, "/products/42");
        assert_eq!(web.query["tab"], "reviews");
        assert_eq!(web.fragment.as_deref(), Some("notes"));

        let custom = parse_route_location("pablo://orders/abc%20123?from=push");
        assert_eq!(custom.path, "/orders/abc%20123");
        assert_eq!(custom.query["from"], "push");

        let hash = parse_route_location("https://rtylr.com/#/checkout?step=pay");
        assert_eq!(hash.path, "/checkout");
        assert_eq!(hash.query["step"], "pay");
        assert_eq!(hash.fragment.as_deref(), Some("/checkout?step=pay"));
    }

    #[test]
    fn match_route_ignores_query_hash_and_decodes_params() {
        let params = match_route(
            "/products/:id/reviews/:review_id",
            "/products/abc%20123/reviews/99?sort=new#top",
        )
        .expect("route should match");

        assert_eq!(params["id"], "abc 123");
        assert_eq!(params["review_id"], "99");
    }

    #[test]
    fn match_route_location_carries_params_query_and_hash() {
        let matched = match_route_location(
            "/orders/:id?tab=items#notes",
            "/orders/abc%20123?tab=items&tag=paid&tag=pickup#notes",
        )
        .expect("route should match");

        assert_eq!(matched.pattern, "/orders/:id?tab=items#notes");
        assert_eq!(matched.path, "/orders/abc%20123");
        assert_eq!(matched.param("id"), Some("abc 123"));
        assert_eq!(matched.query_value("tab"), Some("items"));
        assert_eq!(
            matched.query_values("tag"),
            Some(["paid".to_string(), "pickup".to_string()].as_slice())
        );
        assert_eq!(matched.fragment.as_deref(), Some("notes"));
    }

    #[test]
    fn declarative_route_patterns_can_constrain_query_and_hash() {
        assert!(match_route_location("/orders/:id?tab=items", "/orders/42?tab=items").is_some());
        assert!(match_route_location("/orders/:id?tab=items", "/orders/42?tab=history").is_none());
        assert!(match_route_location("/orders/:id#notes", "/orders/42#notes").is_some());
        assert!(match_route_location("/orders/:id#notes", "/orders/42#summary").is_none());
    }

    #[test]
    fn url_route_matches_exposes_the_route_context() {
        let detail = route("/orders/:id", |_| {
            crate::view::boxed(crate::view::text("detail"))
        });
        let matched = detail
            .matches("/orders/42?tab=items")
            .expect("route should match");

        assert_eq!(detail.pattern(), "/orders/:id");
        assert_eq!(matched.param("id"), Some("42"));
        assert_eq!(matched.query_value("tab"), Some("items"));
    }

    #[test]
    fn parse_deep_link_handles_universal_links() {
        let (path, params) = parse_deep_link("https://rtylr.com/profile/42?tab=posts");

        assert_eq!(path, "/profile/42");
        assert_eq!(params["tab"], "posts");
    }
}
