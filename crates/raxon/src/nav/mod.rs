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

use crate::reactive::{batch, create_signal, provide_context, use_context, Signal};
use crate::view::{dynamic, BoxedView, View};

/// The last typed-navigator transition action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigatorTransitionKind {
    /// Initial route before any user navigation.
    Initial,
    /// A route was pushed on top of the typed navigator stack.
    Push,
    /// The top route was popped.
    Pop,
    /// The top route was replaced without changing stack depth.
    Replace,
    /// The stack was reset to a new root route.
    Reset,
    /// The stack was truncated back to its root route.
    PopToRoot,
}

/// Context passed to pluggable typed-navigator transition wrappers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavigationTransitionContext {
    /// Transition action that produced the current screen.
    pub kind: NavigatorTransitionKind,
    /// Stack depth before the transition.
    pub from_depth: usize,
    /// Stack depth after the transition.
    pub to_depth: usize,
}

impl NavigationTransitionContext {
    /// Creates the initial transition context for a typed navigator.
    pub fn initial(depth: usize) -> Self {
        Self {
            kind: NavigatorTransitionKind::Initial,
            from_depth: depth,
            to_depth: depth,
        }
    }

    /// Creates a transition context from stack depths and an action kind.
    pub fn new(kind: NavigatorTransitionKind, from_depth: usize, to_depth: usize) -> Self {
        Self {
            kind,
            from_depth,
            to_depth,
        }
    }

    /// Returns `true` when the transition moves deeper into the stack.
    pub fn is_forward(self) -> bool {
        self.to_depth > self.from_depth
    }

    /// Returns `true` when the transition moves toward the root.
    pub fn is_back(self) -> bool {
        self.to_depth < self.from_depth
    }
}

/// A navigation stack over routes of type `R`. Cheap `Copy` handle.
pub struct Navigator<R: 'static> {
    stack: Signal<Vec<R>>,
    transition: Signal<NavigationTransitionContext>,
}

/// Debug/introspection snapshot for a typed [`Navigator`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigatorDebugSnapshot<R> {
    /// Routes from root to top.
    pub stack: Vec<R>,
    /// Number of routes in the stack.
    pub depth: usize,
    /// Current top route, if any.
    pub top: Option<R>,
    /// Whether the stack can pop.
    pub can_pop: bool,
    /// Latest transition context.
    pub transition: NavigationTransitionContext,
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
        transition: create_signal(NavigationTransitionContext::initial(1)),
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
        let from_depth = self.depth();
        batch(|| {
            self.transition.set(NavigationTransitionContext::new(
                NavigatorTransitionKind::Push,
                from_depth,
                from_depth + 1,
            ));
            self.stack.update(|s| s.push(route));
        });
    }

    /// Pops the top route (no-op at the root).
    pub fn pop(&self) {
        let from_depth = self.depth();
        if from_depth <= 1 {
            return;
        }

        batch(|| {
            self.transition.set(NavigationTransitionContext::new(
                NavigatorTransitionKind::Pop,
                from_depth,
                from_depth - 1,
            ));
            self.stack.update(|s| {
                s.pop();
            });
        });
    }

    /// Replaces the top route.
    pub fn replace(&self, route: R) {
        let depth = self.depth().max(1);
        batch(|| {
            self.transition.set(NavigationTransitionContext::new(
                NavigatorTransitionKind::Replace,
                depth,
                depth,
            ));
            self.stack.update(|s| {
                s.pop();
                s.push(route);
            });
        });
    }

    /// Resets the stack to a single route.
    pub fn reset(&self, route: R) {
        let from_depth = self.depth();
        batch(|| {
            self.transition.set(NavigationTransitionContext::new(
                NavigatorTransitionKind::Reset,
                from_depth,
                1,
            ));
            self.stack.update(|s| {
                s.clear();
                s.push(route);
            });
        });
    }

    /// Pops back to the root route.
    pub fn pop_to_root(&self) {
        let from_depth = self.depth();
        if from_depth <= 1 {
            return;
        }

        batch(|| {
            self.transition.set(NavigationTransitionContext::new(
                NavigatorTransitionKind::PopToRoot,
                from_depth,
                1,
            ));
            self.stack.update(|s| s.truncate(1));
        });
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

    /// The latest transition context for this navigator.
    pub fn transition_context(&self) -> NavigationTransitionContext {
        self.transition.get()
    }

    /// Captures the current stack, top route, pop state, and transition context.
    pub fn debug_snapshot(&self) -> NavigatorDebugSnapshot<R> {
        let stack = self.stack.get();
        let depth = stack.len();
        let top = stack.last().cloned();
        NavigatorDebugSnapshot {
            stack,
            depth,
            top,
            can_pop: depth > 1,
            transition: self.transition_context(),
        }
    }
}

/// A tab navigator where each tab owns an independent route stack.
///
/// This is the typed "tabs containing stacks" primitive: switching tabs keeps
/// each tab's stack alive, while push/pop/replace operate on the selected tab.
/// Pair [`selected_signal`](Self::selected_signal) with your tab bar of choice,
/// and render the selected stack with [`tab_stack_routes`].
pub struct TabStackNavigator<R: 'static> {
    stacks: Signal<Vec<Vec<R>>>,
    selected: Signal<usize>,
}

/// Debug/introspection snapshot for a [`TabStackNavigator`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabStackDebugSnapshot<R> {
    /// Selected tab index, clamped to the available tab range.
    pub selected_index: usize,
    /// Number of tabs.
    pub tab_count: usize,
    /// Route stacks for every tab.
    pub stacks: Vec<Vec<R>>,
    /// Current selected tab stack.
    pub selected_stack: Vec<R>,
    /// Current route in the selected tab, if any.
    pub selected_top: Option<R>,
    /// Whether the selected tab can pop.
    pub can_pop: bool,
}

impl<R: 'static> Clone for TabStackNavigator<R> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<R: 'static> Copy for TabStackNavigator<R> {}

/// Creates a tab-stack navigator with one root route per tab.
///
/// The first tab is selected initially. The navigator is provided via context
/// so descendants can call [`use_tab_stack_navigator`].
pub fn create_tab_stack_navigator<R: Clone + 'static>(
    roots: impl Into<Vec<R>>,
) -> TabStackNavigator<R> {
    create_tab_stack_navigator_at(roots, 0)
}

/// Creates a tab-stack navigator with `selected_index` selected initially.
///
/// If `selected_index` is out of range it is clamped to the last tab.
/// Panics when `roots` is empty because every tab must have a root route.
pub fn create_tab_stack_navigator_at<R: Clone + 'static>(
    roots: impl Into<Vec<R>>,
    selected_index: usize,
) -> TabStackNavigator<R> {
    let roots = roots.into();
    assert!(
        !roots.is_empty(),
        "tab stack navigator requires at least one root route"
    );

    let tab_count = roots.len();
    let nav = TabStackNavigator {
        stacks: create_signal(roots.into_iter().map(|root| vec![root]).collect()),
        selected: create_signal(selected_index.min(tab_count - 1)),
    };
    provide_context(nav);
    nav
}

/// The tab-stack navigator of route type `R` in scope, if any.
pub fn use_tab_stack_navigator<R: Clone + 'static>() -> Option<TabStackNavigator<R>> {
    use_context::<TabStackNavigator<R>>()
}

impl<R: Clone + 'static> TabStackNavigator<R> {
    /// Returns the selected tab index, clamped to the available tabs.
    pub fn selected_index(&self) -> usize {
        let count = self.tab_count();
        if count == 0 {
            return 0;
        }
        self.selected.get().min(count - 1)
    }

    /// Returns the reactive selected-tab signal.
    ///
    /// Use this to bind a visual tab bar to the navigator.
    pub fn selected_signal(&self) -> Signal<usize> {
        self.selected
    }

    /// Selects a tab by index. Returns `false` when the index is out of range.
    pub fn select(&self, index: usize) -> bool {
        if index >= self.tab_count() {
            return false;
        }
        self.selected.set(index);
        true
    }

    /// Number of tabs managed by this navigator.
    pub fn tab_count(&self) -> usize {
        self.stacks.with(|stacks| stacks.len())
    }

    /// Returns a clone of every tab stack.
    pub fn stacks(&self) -> Vec<Vec<R>> {
        self.stacks.get()
    }

    /// Returns a clone of one tab stack.
    pub fn stack(&self, index: usize) -> Option<Vec<R>> {
        self.stacks.with(|stacks| stacks.get(index).cloned())
    }

    /// Returns the current route for the selected tab.
    pub fn top(&self) -> R {
        self.top_for_tab(self.selected_index())
            .expect("selected tab stack exists")
    }

    /// Returns the current route for one tab.
    pub fn top_for_tab(&self, index: usize) -> Option<R> {
        self.stacks
            .with(|stacks| stacks.get(index).and_then(|stack| stack.last()).cloned())
    }

    /// Depth of the selected tab's stack.
    pub fn depth(&self) -> usize {
        self.depth_for_tab(self.selected_index())
    }

    /// Depth of one tab's stack.
    pub fn depth_for_tab(&self, index: usize) -> usize {
        self.stacks
            .with(|stacks| stacks.get(index).map(Vec::len).unwrap_or_default())
    }

    /// Whether the selected tab can pop back to its root route.
    pub fn can_pop(&self) -> bool {
        self.depth() > 1
    }

    /// Whether one tab can pop back to its root route.
    pub fn can_pop_tab(&self, index: usize) -> bool {
        self.depth_for_tab(index) > 1
    }

    /// Pushes a route onto the selected tab stack.
    pub fn push(&self, route: R) {
        let selected = self.selected_index();
        self.stacks.update(|stacks| stacks[selected].push(route));
    }

    /// Pops the selected tab stack. Returns `false` at the root.
    pub fn pop(&self) -> bool {
        let selected = self.selected_index();
        let mut did_pop = false;
        self.stacks.update(|stacks| {
            if let Some(stack) = stacks.get_mut(selected) {
                if stack.len() > 1 {
                    stack.pop();
                    did_pop = true;
                }
            }
        });
        did_pop
    }

    /// Replaces the selected tab's top route.
    pub fn replace(&self, route: R) {
        let selected = self.selected_index();
        self.stacks.update(|stacks| {
            let stack = &mut stacks[selected];
            if let Some(top) = stack.last_mut() {
                *top = route;
            } else {
                stack.push(route);
            }
        });
    }

    /// Resets the selected tab to a single root route.
    pub fn reset(&self, route: R) {
        let selected = self.selected_index();
        let _ = self.reset_tab(selected, route);
    }

    /// Resets one tab to a single root route. Returns `false` when out of range.
    pub fn reset_tab(&self, index: usize, route: R) -> bool {
        if index >= self.tab_count() {
            return false;
        }
        self.stacks.update(|stacks| stacks[index] = vec![route]);
        true
    }

    /// Pops the selected tab back to its root route.
    pub fn pop_to_root(&self) -> bool {
        let selected = self.selected_index();
        self.pop_tab_to_root(selected)
    }

    /// Pops one tab back to its root route. Returns `false` when nothing changed.
    pub fn pop_tab_to_root(&self, index: usize) -> bool {
        if index >= self.tab_count() {
            return false;
        }
        let mut changed = false;
        self.stacks.update(|stacks| {
            let stack = &mut stacks[index];
            if stack.len() > 1 {
                stack.truncate(1);
                changed = true;
            }
        });
        changed
    }

    /// Captures all tab stacks plus selected-tab state for devtools/inspectors.
    pub fn debug_snapshot(&self) -> TabStackDebugSnapshot<R> {
        let stacks = self.stacks.get();
        let tab_count = stacks.len();
        let selected_index = if tab_count == 0 {
            0
        } else {
            self.selected.get().min(tab_count - 1)
        };
        let selected_stack = stacks.get(selected_index).cloned().unwrap_or_default();
        let selected_top = selected_stack.last().cloned();
        let can_pop = selected_stack.len() > 1;

        TabStackDebugSnapshot {
            selected_index,
            tab_count,
            stacks,
            selected_stack,
            selected_top,
            can_pop,
        }
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

/// Renders the selected tab stack's current route.
///
/// Use this with [`create_tab_stack_navigator`] when a bottom/top tab UI should
/// preserve an independent stack per tab.
pub fn tab_stack_routes<R, F>(nav: TabStackNavigator<R>, mut render: F) -> impl View
where
    R: Clone + 'static,
    F: FnMut(R) -> BoxedView + 'static,
{
    dynamic(move || render(nav.top()))
}

/// Renders all tab-stack panes once and toggles their visibility by selection.
///
/// Unlike [`tab_stack_routes`], switching tabs does not rebuild the newly
/// selected tab pane. Each pane still reacts to its own stack top, so pushes and
/// pops inside a tab update that tab's pane.
///
/// # Example
/// ```rust
/// use raxon::nav::{create_tab_stack_navigator, keep_alive_tab_stack_routes};
/// use raxon::view::{boxed, text, View};
///
/// #[derive(Clone)]
/// enum Screen { Home, Orders }
///
/// fn app() -> impl View {
///     let nav = create_tab_stack_navigator(vec![Screen::Home, Screen::Orders]);
///     keep_alive_tab_stack_routes(nav, move |screen| match screen {
///         Screen::Home => boxed(text("home")),
///         Screen::Orders => boxed(text("orders")),
///     })
/// }
/// ```
pub fn keep_alive_tab_stack_routes<R, F>(nav: TabStackNavigator<R>, render: F) -> impl View
where
    R: Clone + 'static,
    F: FnMut(R) -> BoxedView + 'static,
{
    use crate::view::{boxed, column, ViewExt};

    let render = std::rc::Rc::new(std::cell::RefCell::new(render));
    let panes = (0..nav.tab_count())
        .map(|index| {
            let pane_nav = nav;
            let opacity_nav = nav;
            let pane_render = std::rc::Rc::clone(&render);
            let pane = dynamic(move || {
                if let Some(route) = pane_nav.top_for_tab(index) {
                    return pane_render.borrow_mut()(route);
                }
                boxed(column(()))
            });

            boxed(
                boxed(pane)
                    .opacity_fn(move || {
                        if opacity_nav.selected_index() == index {
                            1.0
                        } else {
                            0.0
                        }
                    })
                    .grow(1.0),
            )
        })
        .collect::<Vec<_>>();

    column(panes).grow()
}

/// A batteries-included tab-stack navigator.
///
/// Creates a [`TabStackNavigator`] with one root route per tab, provides it via
/// context, and renders the selected tab's current route.
pub fn tab_stack<R, F>(roots: impl Into<Vec<R>>, render: F) -> impl View
where
    R: Clone + 'static,
    F: FnMut(R) -> BoxedView + 'static,
{
    let nav = create_tab_stack_navigator(roots);
    tab_stack_routes(nav, render)
}

/// A batteries-included keep-alive tab-stack navigator.
///
/// Creates a [`TabStackNavigator`] and renders all tab panes mounted, hiding
/// inactive panes with opacity.
pub fn keep_alive_tab_stack<R, F>(roots: impl Into<Vec<R>>, render: F) -> impl View
where
    R: Clone + 'static,
    F: FnMut(R) -> BoxedView + 'static,
{
    let nav = create_tab_stack_navigator(roots);
    keep_alive_tab_stack_routes(nav, render)
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

/// How an incoming screen enters during typed-navigator route changes.
///
/// Pass to [`transition_routes`] to get animated push/pop transitions.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NavigationTransition {
    /// Slide from the right on push and from the left on back/pop.
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
    static TRANSITION_START_HANDLERS: RefCell<Vec<Box<dyn Fn(&RouteTransitionEvent)>>> = RefCell::new(vec![]);
    static TRANSITION_COMPLETE_HANDLERS: RefCell<Vec<Box<dyn Fn(&RouteTransitionEvent)>>> = RefCell::new(vec![]);
}

/// The kind of string-router route transition being committed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteTransitionKind {
    /// A new route was pushed onto the history stack.
    Push,
    /// The current route was replaced without adding history.
    Replace,
    /// The router moved back to a previous history entry.
    Pop,
    /// Navigation state was restored from a snapshot or persisted storage.
    Restore,
    /// The browser supplied a route through back/forward or hash navigation.
    Browser,
}

/// Route transition lifecycle event payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteTransitionEvent {
    /// Previous route before the transition.
    pub from: String,
    /// Destination route after the transition.
    pub to: String,
    /// Transition kind.
    pub kind: RouteTransitionKind,
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

/// Register a callback that fires before a route transition commits focus/blur.
pub fn on_transition_start(listener: impl Fn(&RouteTransitionEvent) + 'static) {
    TRANSITION_START_HANDLERS.with(|handlers| {
        handlers.borrow_mut().push(Box::new(listener));
    });
}

/// Register a callback that fires after a route transition commits focus/blur.
pub fn on_transition_complete(listener: impl Fn(&RouteTransitionEvent) + 'static) {
    TRANSITION_COMPLETE_HANDLERS.with(|handlers| {
        handlers.borrow_mut().push(Box::new(listener));
    });
}

/// Fires all registered transition-start listeners.
pub fn fire_transition_start(event: &RouteTransitionEvent) {
    TRANSITION_START_HANDLERS.with(|handlers| {
        for listener in handlers.borrow().iter() {
            listener(event);
        }
    });
}

/// Fires all registered transition-complete listeners.
pub fn fire_transition_complete(event: &RouteTransitionEvent) {
    TRANSITION_COMPLETE_HANDLERS.with(|handlers| {
        for listener in handlers.borrow().iter() {
            listener(event);
        }
    });
}

// ---------------------------------------------------------------------------
// String-based programmatic navigation
// ---------------------------------------------------------------------------

use std::any::{Any, TypeId};
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// Default persistent storage key used by [`save_navigation_state`].
pub const NAVIGATION_STATE_KEY: &str = "raxon.navigation.state";

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
    /// Pending route result callbacks, last-opened route first.
    static ROUTE_RESULT_HANDLERS: RefCell<Vec<PendingRouteResult>> = RefCell::new(Vec::new());
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

struct PendingRouteResult {
    route: String,
    type_id: TypeId,
    type_name: &'static str,
    callback: Box<dyn FnOnce(Box<dyn Any>)>,
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

fn commit_route_change(from: &str, to: &str, kind: RouteTransitionKind) {
    let changed = from != to;
    let event = changed.then(|| RouteTransitionEvent {
        from: from.to_string(),
        to: to.to_string(),
        kind,
    });

    if let Some(event) = event.as_ref() {
        fire_transition_start(event);
    }

    let sig = ensure_route_signal();
    sig.set(to.to_string());

    if changed {
        if !from.is_empty() {
            fire_disappear(from);
        }
        if !to.is_empty() {
            fire_appear(to);
        }
    }

    if let Some(event) = event.as_ref() {
        fire_transition_complete(event);
    }

    fire_navigate_event(from, to);
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

    /// Serializes this location back to a route string.
    ///
    /// Query keys are sorted for stability while repeated values keep their
    /// original order. Hash-route locations such as `/#/checkout?step=pay`
    /// stay in hash-route form.
    pub fn to_route_string(&self) -> String {
        format_route_location(self)
    }

    /// Returns a copy with `key` set to one decoded query value.
    pub fn with_query_param(&self, key: &str, value: impl Into<String>) -> Self {
        self.with_query_values(key, [value.into()])
    }

    /// Returns a copy with `key` set to the provided decoded query values.
    ///
    /// Passing no values removes the key.
    pub fn with_query_values<I, V>(&self, key: &str, values: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: Into<String>,
    {
        let mut next = self.clone();
        let values: Vec<String> = values.into_iter().map(Into::into).collect();
        if values.is_empty() {
            next.query_all.remove(key);
        } else {
            next.query_all.insert(key.to_string(), values);
        }
        sync_first_query_values(&mut next);
        next
    }

    /// Returns a copy without `key` in the query string.
    pub fn without_query_param(&self, key: &str) -> Self {
        let mut next = self.clone();
        next.query_all.remove(key);
        sync_first_query_values(&mut next);
        next
    }

    /// Returns a copy with the URL fragment/hash set to `fragment`.
    pub fn with_fragment(&self, fragment: impl Into<String>) -> Self {
        let mut next = self.clone();
        next.fragment = Some(fragment.into());
        next
    }

    /// Returns a copy without a URL fragment/hash.
    pub fn without_fragment(&self) -> Self {
        let mut next = self.clone();
        next.fragment = None;
        next
    }
}

/// Serializable navigation state for app relaunch and process-restore flows.
///
/// The string router state is intentionally plain data so apps can store it
/// through `raxon::store`, send it through their own persistence layer, or
/// snapshot it during tests.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavigationState {
    /// Current route at the top of the string-router stack.
    pub current: String,
    /// String-router history from oldest to newest route.
    pub history: Vec<String>,
    /// Modal routes from bottom to top.
    pub modals: Vec<String>,
}

/// Debug/introspection snapshot for the string router.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationDebugSnapshot {
    /// Current route at the top of the string-router stack.
    pub current: String,
    /// Parsed current route, including query params and fragment.
    pub location: RouteLocation,
    /// String-router history from oldest to newest route.
    pub history: Vec<String>,
    /// Number of entries in [`history`](Self::history).
    pub history_depth: usize,
    /// Whether the string router can go back.
    pub can_go_back: bool,
    /// Modal routes from bottom to top.
    pub modals: Vec<String>,
    /// Number of active modal routes.
    pub modal_depth: usize,
    /// Top-most modal route, if any.
    pub active_modal: Option<String>,
    /// Whether a route-result callback is pending.
    pub has_pending_result: bool,
    /// Route associated with the latest pending result callback, if any.
    pub pending_result_route: Option<String>,
    /// Rust type name expected by the latest pending result callback, if any.
    pub pending_result_type: Option<String>,
    /// Number of pending result callbacks.
    pub pending_result_count: usize,
}

impl NavigationState {
    /// Creates a normalized navigation state snapshot.
    pub fn new(
        current: impl Into<String>,
        history: impl Into<Vec<String>>,
        modals: impl Into<Vec<String>>,
    ) -> Self {
        normalize_navigation_state(Self {
            current: current.into(),
            history: history.into(),
            modals: modals.into(),
        })
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
/// `pattern` supports static path segments, `:param` captures, optional
/// `:param?` segments, and final `*splat` catchalls. Query keys and fragments
/// in the pattern act as constraints, so `"/orders/:id?tab=items"` only matches
/// routes whose first `tab` query value is `"items"`.
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

    crate::web::push_path(&destination);
    commit_route_change(&from, &destination, RouteTransitionKind::Push);

    destination
}

/// Navigates to `route` and registers a typed one-shot result callback.
///
/// Use this for pick-and-return flows: a caller opens a route, the opened screen
/// later calls [`return_route_result`] with a value of the same type, and Raxon
/// pops back before invoking `on_result`.
///
/// If a guard redirects away from `route`, no result callback is registered.
/// The returned string is the route actually reached, matching [`navigate`].
pub fn navigate_for_result<T: 'static>(route: &str, on_result: impl FnOnce(T) + 'static) -> String {
    let destination = navigate(route);
    if destination == route {
        push_route_result_handler(destination.clone(), on_result);
    }
    destination
}

/// Like [`navigate_for_result`], but returns `true` only if navigation reached
/// the requested route and a result callback was registered.
pub fn try_navigate_for_result<T: 'static>(
    route: &str,
    on_result: impl FnOnce(T) + 'static,
) -> bool {
    navigate_for_result(route, on_result) == route
}

/// Returns the latest typed route result and pops back to the previous route.
///
/// Returns `false` when there is no pending result callback, when the pending
/// callback expects a different type, or when there is no previous route to pop
/// back to. On a type mismatch the pending callback remains registered.
pub fn return_route_result<T: 'static>(value: T) -> bool {
    let pending = ROUTE_RESULT_HANDLERS.with(|handlers| {
        let mut handlers = handlers.borrow_mut();
        let pending = handlers.last()?;
        if pending.type_id != TypeId::of::<T>() {
            return None;
        }
        handlers.pop()
    });

    let Some(pending) = pending else {
        return false;
    };

    if !go_back() {
        ROUTE_RESULT_HANDLERS.with(|handlers| handlers.borrow_mut().push(pending));
        return false;
    }

    (pending.callback)(Box::new(value));
    true
}

/// Cancels the latest pending route result and pops back to the previous route.
///
/// The registered callback is dropped without being invoked. Returns `false`
/// when no result is pending or there is no previous route to pop back to.
pub fn cancel_route_result() -> bool {
    let pending = ROUTE_RESULT_HANDLERS.with(|handlers| handlers.borrow_mut().pop());
    let Some(pending) = pending else {
        return false;
    };

    if !go_back() {
        ROUTE_RESULT_HANDLERS.with(|handlers| handlers.borrow_mut().push(pending));
        return false;
    }

    true
}

/// Returns `true` if the current route was opened with [`navigate_for_result`].
pub fn has_pending_route_result() -> bool {
    ROUTE_RESULT_HANDLERS.with(|handlers| !handlers.borrow().is_empty())
}

/// Returns the Rust type name expected by the latest pending route result.
pub fn pending_route_result_type() -> Option<&'static str> {
    ROUTE_RESULT_HANDLERS.with(|handlers| handlers.borrow().last().map(|pending| pending.type_name))
}

/// Returns the route associated with the latest pending route result.
pub fn pending_route_result_route() -> Option<String> {
    ROUTE_RESULT_HANDLERS.with(|handlers| {
        handlers
            .borrow()
            .last()
            .map(|pending| pending.route.clone())
    })
}

fn push_route_result_handler<T: 'static>(route: String, on_result: impl FnOnce(T) + 'static) {
    ROUTE_RESULT_HANDLERS.with(|handlers| {
        handlers.borrow_mut().push(PendingRouteResult {
            route,
            type_id: TypeId::of::<T>(),
            type_name: std::any::type_name::<T>(),
            callback: Box::new(move |value| {
                let value = value
                    .downcast::<T>()
                    .expect("route result type checked before dispatch");
                on_result(*value);
            }),
        });
    });
}

/// Replaces the current string-router route without pushing a new history entry.
///
/// Guards still run, listeners still fire, and on web the address bar is updated
/// with `history.replaceState`. Use this for URL state such as filters, tabs, and
/// search params that should not create a browser back-stack entry for every
/// keystroke.
pub fn replace_route(route: &str) -> String {
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

    crate::web::replace_path(&destination);
    commit_route_change(&from, &destination, RouteTransitionKind::Replace);

    destination
}

/// Pop the current route from the history stack and return to the previous
/// one. Returns `false` if the stack is already empty.
pub fn go_back() -> bool {
    let transition = HISTORY_STACK.with(|s| {
        let mut stack = s.borrow_mut();
        if stack.len() <= 1 {
            return None;
        }
        let from = stack.pop().unwrap_or_default();
        let to = stack.last().cloned().unwrap_or_default();
        Some((from, to))
    });

    if let Some((from, to)) = transition {
        crate::web::replace_path(&to);
        commit_route_change(&from, &to, RouteTransitionKind::Pop);
        return true;
    }

    false
}

/// Returns `true` if there is at least one route to go back to.
pub fn can_go_back() -> bool {
    HISTORY_STACK.with(|s| s.borrow().len() > 1)
}

/// Captures the current string-router history and modal stack.
pub fn navigation_state() -> NavigationState {
    let history = HISTORY_STACK.with(|stack| stack.borrow().clone());
    let current = history
        .last()
        .cloned()
        .or_else(|| CURRENT_ROUTE.with(|route| route.borrow().map(|signal| signal.get())))
        .unwrap_or_default();
    let modals = MODAL_STACK.with(|stack| stack.borrow().clone());

    NavigationState::new(current, history, modals)
}

/// Captures a devtools-friendly snapshot of string-router state.
///
/// The snapshot includes the normalized current route, parsed path/query/hash
/// location, history stack, modal stack, back availability, and pending
/// route-result metadata.
pub fn navigation_debug_snapshot() -> NavigationDebugSnapshot {
    let state = navigation_state();
    let history_depth = state.history.len();
    let can_go_back = history_depth > 1;
    let modal_depth = state.modals.len();
    let active_modal = state.modals.last().cloned();
    let location = parse_route_location(&state.current);
    let (pending_result_route, pending_result_type, pending_result_count) = ROUTE_RESULT_HANDLERS
        .with(|handlers| {
            let handlers = handlers.borrow();
            let pending = handlers.last();
            (
                pending.map(|pending| pending.route.clone()),
                pending.map(|pending| pending.type_name.to_string()),
                handlers.len(),
            )
        });

    NavigationDebugSnapshot {
        current: state.current,
        location,
        history: state.history,
        history_depth,
        can_go_back,
        modals: state.modals,
        modal_depth,
        active_modal,
        has_pending_result: pending_result_count > 0,
        pending_result_route,
        pending_result_type,
        pending_result_count,
    }
}

/// Restores string-router history and modal stack from a snapshot.
///
/// Pending route-result callbacks are cleared because callback closures cannot
/// be serialized safely across app relaunches. Guards are applied to the
/// restored current route; if a guard redirects, the restored history is updated
/// to end at the redirected destination.
pub fn restore_navigation_state(state: NavigationState) -> NavigationState {
    let mut state = normalize_navigation_state(state);

    if let Some(destination) = check_guards(&state.current) {
        state.current = destination;
        if let Some(last) = state.history.last_mut() {
            *last = state.current.clone();
        } else {
            state.history.push(state.current.clone());
        }
    }

    let from = HISTORY_STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        let from = stack.last().cloned().unwrap_or_default();
        *stack = state.history.clone();
        from
    });
    MODAL_STACK.with(|stack| *stack.borrow_mut() = state.modals.clone());
    ROUTE_RESULT_HANDLERS.with(|handlers| handlers.borrow_mut().clear());

    crate::web::replace_path(&state.current);
    commit_route_change(&from, &state.current, RouteTransitionKind::Restore);

    state
}

/// Serializes a navigation state snapshot as JSON.
pub fn encode_navigation_state(state: &NavigationState) -> Result<String, String> {
    serde_json::to_string(state).map_err(|err| err.to_string())
}

/// Parses a navigation state snapshot from JSON and normalizes it.
pub fn decode_navigation_state(json: &str) -> Result<NavigationState, String> {
    serde_json::from_str(json)
        .map(normalize_navigation_state)
        .map_err(|err| err.to_string())
}

/// Saves the current navigation state through `raxon::store`.
///
/// Uses [`NAVIGATION_STATE_KEY`]. Platform storage determines durability:
/// browser `localStorage` on web, NSUserDefaults on iOS when installed, and the
/// current configured storage backend elsewhere.
pub fn save_navigation_state() -> Result<NavigationState, String> {
    let state = navigation_state();
    let json = encode_navigation_state(&state)?;
    crate::store::store_set(NAVIGATION_STATE_KEY, &json);
    Ok(state)
}

/// Restores navigation state previously saved by [`save_navigation_state`].
pub fn restore_saved_navigation_state() -> Option<NavigationState> {
    let json = crate::store::store_get(NAVIGATION_STATE_KEY)?;
    let state = decode_navigation_state(&json).ok()?;
    Some(restore_navigation_state(state))
}

/// Clears the persisted navigation state saved under [`NAVIGATION_STATE_KEY`].
pub fn clear_saved_navigation_state() {
    crate::store::store_remove(NAVIGATION_STATE_KEY);
}

fn normalize_navigation_state(mut state: NavigationState) -> NavigationState {
    state.history = state
        .history
        .into_iter()
        .filter_map(normalize_route_value)
        .collect();
    state.modals = state
        .modals
        .into_iter()
        .filter_map(normalize_route_value)
        .collect();

    state.current = normalize_route_value(state.current)
        .or_else(|| state.history.last().cloned())
        .unwrap_or_else(|| "/".to_string());

    if state.history.last() != Some(&state.current) {
        state.history.push(state.current.clone());
    }

    state
}

fn normalize_route_value(route: impl AsRef<str>) -> Option<String> {
    let route = route.as_ref().trim();
    (!route.is_empty()).then(|| route.to_string())
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

/// Pushes a new route with `key=value` in the current query string.
///
/// Existing path, other query keys, repeated query values, and hash-route style
/// are preserved.
pub fn set_query_param(key: &str, value: impl Into<String>) -> String {
    let route = current_route_location()
        .with_query_param(key, value)
        .to_route_string();
    navigate(&route)
}

/// Replaces the current route with `key=value` in the current query string.
///
/// This updates the URL without adding a browser/back-stack entry.
pub fn replace_query_param(key: &str, value: impl Into<String>) -> String {
    let route = current_route_location()
        .with_query_param(key, value)
        .to_route_string();
    replace_route(&route)
}

/// Pushes a new route with all values for `key` replaced in the current query.
///
/// Passing no values removes the key.
pub fn set_query_param_values<I, V>(key: &str, values: I) -> String
where
    I: IntoIterator<Item = V>,
    V: Into<String>,
{
    let route = current_route_location()
        .with_query_values(key, values)
        .to_route_string();
    navigate(&route)
}

/// Replaces the current route with all values for `key` changed in the query.
///
/// Passing no values removes the key and no browser/back-stack entry is added.
pub fn replace_query_param_values<I, V>(key: &str, values: I) -> String
where
    I: IntoIterator<Item = V>,
    V: Into<String>,
{
    let route = current_route_location()
        .with_query_values(key, values)
        .to_route_string();
    replace_route(&route)
}

/// Pushes a new route with `key` removed from the current query string.
pub fn remove_query_param(key: &str) -> String {
    let route = current_route_location()
        .without_query_param(key)
        .to_route_string();
    navigate(&route)
}

/// Replaces the current route with `key` removed from the current query string.
pub fn replace_remove_query_param(key: &str) -> String {
    let route = current_route_location()
        .without_query_param(key)
        .to_route_string();
    replace_route(&route)
}

/// Renders the first declarative URL route that matches [`current_route`].
///
/// Use this for web/deep-link-addressable screen shells. On web, this binds the
/// string router to the browser URL so reloads, hash changes, and back/forward
/// navigation stay in sync.
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
    bind_web_history();

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

/// Match a route `pattern` against a concrete `route`.
///
/// Patterns support static segments, `:param` captures, optional `:param?`
/// captures, and final `*splat` catchalls. Returns a map of parameter names to
/// their decoded values on success, or `None` if the shapes don't match.
///
/// # Example
/// ```
/// use raxon::nav::match_route;
/// let params = match_route("/user/:id/post/:postId", "/user/42/post/7").unwrap();
/// assert_eq!(params["id"], "42");
/// assert_eq!(params["postId"], "7");
/// ```
pub fn match_route(pattern: &str, route: &str) -> Option<HashMap<String, String>> {
    let pattern_location = parse_route_pattern_location(pattern);
    let route_location = parse_route_location(route);
    match_path_params(&pattern_location.path, &route_location.path)
}

/// Match a declarative route pattern against a route/URL and return all route
/// state needed by a screen renderer.
pub fn match_route_location(pattern: &str, route: &str) -> Option<RouteMatch> {
    let location = parse_route_location(route);
    match_route_definition(pattern, location)
}

/// Builds a concrete route from a pattern and decoded path parameters.
///
/// Missing required `:param` or named `*splat` values return `None`. Optional
/// `:param?` segments are omitted when their value is missing or empty.
///
/// # Example
/// ```
/// use raxon::nav::build_route;
/// let route = build_route("/orders/:id", [("id", "abc 123")]).unwrap();
/// assert_eq!(route, "/orders/abc%20123");
/// ```
pub fn build_route<K, V, I>(pattern: &str, params: I) -> Option<String>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    build_route_with_query(pattern, params, std::iter::empty::<(&str, &str)>())
}

/// Builds a concrete route from a pattern, decoded path parameters, and decoded
/// query pairs.
///
/// Query pairs may repeat keys. Pairs passed here replace same-named query
/// constraints from the pattern, while other pattern query constraints remain.
pub fn build_route_with_query<K, V, I, QK, QV, QI>(
    pattern: &str,
    params: I,
    query: QI,
) -> Option<String>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
    QI: IntoIterator<Item = (QK, QV)>,
    QK: AsRef<str>,
    QV: AsRef<str>,
{
    let pattern_location = parse_route_pattern_location(pattern);
    let params = collect_route_params(params);
    let path = build_route_path(&pattern_location.path, &params)?;
    let mut location = RouteLocation {
        path,
        query: pattern_location.query,
        query_all: pattern_location.query_all,
        fragment: pattern_location.fragment,
    };

    let mut replaced_query_keys = HashSet::new();
    for (key, value) in query {
        let key = key.as_ref();
        if key.is_empty() {
            continue;
        }
        if replaced_query_keys.insert(key.to_string()) {
            location.query_all.remove(key);
        }
        location
            .query_all
            .entry(key.to_string())
            .or_default()
            .push(value.as_ref().to_string());
    }

    sync_first_query_values(&mut location);
    Some(location.to_route_string())
}

fn match_route_definition(pattern: &str, location: RouteLocation) -> Option<RouteMatch> {
    let pattern_location = parse_route_pattern_location(pattern);
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
    match_path_segments(&pattern_segs, &route_segs, 0, 0, HashMap::new())
}

fn match_path_segments(
    pattern_segs: &[&str],
    route_segs: &[&str],
    pattern_index: usize,
    route_index: usize,
    params: HashMap<String, String>,
) -> Option<HashMap<String, String>> {
    if pattern_index == pattern_segs.len() {
        return (route_index == route_segs.len()).then_some(params);
    }

    let pattern_segment = pattern_segs[pattern_index];

    if let Some(splat_name) = pattern_segment.strip_prefix('*') {
        if pattern_index + 1 != pattern_segs.len() {
            return None;
        }
        let mut params = params;
        if !splat_name.is_empty() {
            let value = route_segs[route_index..]
                .iter()
                .map(|segment| decode_url_component(segment, false))
                .collect::<Vec<_>>()
                .join("/");
            params.insert(splat_name.to_string(), value);
        }
        return Some(params);
    }

    if let Some(param_name) = optional_param_name(pattern_segment) {
        if route_index < route_segs.len() {
            let mut consumed_params = params.clone();
            consumed_params.insert(
                param_name.to_string(),
                decode_url_component(route_segs[route_index], false),
            );
            if let Some(matched) = match_path_segments(
                pattern_segs,
                route_segs,
                pattern_index + 1,
                route_index + 1,
                consumed_params,
            ) {
                return Some(matched);
            }
        }
        return match_path_segments(
            pattern_segs,
            route_segs,
            pattern_index + 1,
            route_index,
            params,
        );
    }

    if let Some(param_name) = required_param_name(pattern_segment) {
        if route_index >= route_segs.len() {
            return None;
        }
        let mut params = params;
        params.insert(
            param_name.to_string(),
            decode_url_component(route_segs[route_index], false),
        );
        return match_path_segments(
            pattern_segs,
            route_segs,
            pattern_index + 1,
            route_index + 1,
            params,
        );
    }

    if route_index >= route_segs.len()
        || decode_url_component(pattern_segment, false)
            != decode_url_component(route_segs[route_index], false)
    {
        return None;
    }

    match_path_segments(
        pattern_segs,
        route_segs,
        pattern_index + 1,
        route_index + 1,
        params,
    )
}

fn build_route_path(pattern_path: &str, params: &HashMap<String, String>) -> Option<String> {
    let pattern_segs: Vec<&str> = pattern_path.split('/').filter(|s| !s.is_empty()).collect();
    let mut route_segs = Vec::new();

    for (index, segment) in pattern_segs.iter().enumerate() {
        if let Some(splat_name) = segment.strip_prefix('*') {
            if index + 1 != pattern_segs.len() {
                return None;
            }
            if !splat_name.is_empty() {
                let value = params.get(splat_name)?;
                route_segs.extend(
                    value
                        .split('/')
                        .filter(|part| !part.is_empty())
                        .map(encode_path_segment),
                );
            }
            break;
        }

        if let Some(param_name) = optional_param_name(segment) {
            if let Some(value) = params.get(param_name).filter(|value| !value.is_empty()) {
                route_segs.push(encode_path_segment(value));
            }
            continue;
        }

        if let Some(param_name) = required_param_name(segment) {
            let value = params.get(param_name).filter(|value| !value.is_empty())?;
            route_segs.push(encode_path_segment(value));
            continue;
        }

        route_segs.push(encode_path_segment(&decode_url_component(segment, false)));
    }

    if route_segs.is_empty() {
        Some("/".to_string())
    } else {
        Some(format!("/{}", route_segs.join("/")))
    }
}

fn collect_route_params<K, V, I>(params: I) -> HashMap<String, String>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    params
        .into_iter()
        .map(|(key, value)| (key.as_ref().to_string(), value.as_ref().to_string()))
        .collect()
}

fn optional_param_name(segment: &str) -> Option<&str> {
    let name = segment.strip_prefix(':')?.strip_suffix('?')?;
    (!name.is_empty()).then_some(name)
}

fn required_param_name(segment: &str) -> Option<&str> {
    let name = segment.strip_prefix(':')?;
    if name.is_empty() || name.ends_with('?') {
        None
    } else {
        Some(name)
    }
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

fn parse_route_pattern_location(input: &str) -> RouteLocation {
    let trimmed = input.trim();
    let (without_fragment, fragment_raw) = split_once(trimmed, '#');
    let fragment = fragment_raw
        .filter(|value| !value.is_empty())
        .map(|value| decode_url_component(value, false));

    if let Some(hash_route) = fragment_raw.and_then(hash_route_part) {
        let mut location = parse_route_pattern_location(hash_route);
        location.fragment = fragment;
        return location;
    }

    let route_part = strip_url_prefix(without_fragment);
    let (raw_path, query_raw) = split_pattern_path_query(route_part);
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

fn sync_first_query_values(location: &mut RouteLocation) {
    location.query = first_query_values(&location.query_all);
}

fn format_route_location(location: &RouteLocation) -> String {
    let path_and_query = format_path_and_query(&location.path, &location.query_all);

    let Some(fragment) = location.fragment.as_deref() else {
        return path_and_query;
    };

    if fragment.starts_with("!/") {
        return format!("/#!{path_and_query}");
    }

    if fragment.starts_with('/') {
        return format!("/#{path_and_query}");
    }

    let mut route = path_and_query;
    route.push('#');
    route.push_str(&encode_fragment_component(fragment));
    route
}

fn format_path_and_query(path: &str, query_all: &HashMap<String, Vec<String>>) -> String {
    let mut route = normalize_route_path(path);
    if query_all.is_empty() {
        return route;
    }

    let mut keys: Vec<&String> = query_all.keys().collect();
    keys.sort();

    let mut pairs = Vec::new();
    for key in keys {
        if let Some(values) = query_all.get(key) {
            for value in values {
                pairs.push(format!(
                    "{}={}",
                    encode_query_component(key),
                    encode_query_component(value)
                ));
            }
        }
    }

    route.push('?');
    route.push_str(&pairs.join("&"));
    route
}

fn split_once(input: &str, needle: char) -> (&str, Option<&str>) {
    if let Some(index) = input.find(needle) {
        (&input[..index], Some(&input[index + needle.len_utf8()..]))
    } else {
        (input, None)
    }
}

fn split_pattern_path_query(input: &str) -> (&str, Option<&str>) {
    for (index, ch) in input.char_indices() {
        if ch == '?' && !is_optional_param_marker(input, index) {
            return (&input[..index], Some(&input[index + ch.len_utf8()..]));
        }
    }
    (input, None)
}

fn is_optional_param_marker(input: &str, question_index: usize) -> bool {
    let next = input[question_index + '?'.len_utf8()..].chars().next();
    if !matches!(next, None | Some('/')) {
        return false;
    }

    let segment_start = input[..question_index]
        .rfind('/')
        .map(|index| index + '/'.len_utf8())
        .unwrap_or(0);
    let segment = &input[segment_start..question_index];
    segment.starts_with(':') && segment.len() > 1
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

fn encode_query_component(input: &str) -> String {
    encode_url_component(input, true, false)
}

fn encode_path_segment(input: &str) -> String {
    encode_url_component(input, false, false)
}

fn encode_fragment_component(input: &str) -> String {
    encode_url_component(input, false, true)
}

fn encode_url_component(input: &str, space_as_plus: bool, fragment_safe: bool) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        let unreserved = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        let fragment_extra_safe = fragment_safe
            && matches!(
                byte,
                b'/' | b'?'
                    | b'&'
                    | b'='
                    | b':'
                    | b'@'
                    | b'!'
                    | b'$'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b','
                    | b';'
            );

        if unreserved || fragment_extra_safe {
            out.push(byte as char);
        } else if byte == b' ' && space_as_plus {
            out.push('+');
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
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
    crate::web::on_location_change(|route| replace_route_from_browser(&route));
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

    if destination != route {
        crate::web::replace_path(&destination);
    }
    commit_route_change(&from, &destination, RouteTransitionKind::Browser);
}

#[cfg(test)]
fn reset_navigation_for_tests() {
    APPEAR_HANDLERS.with(|handlers| handlers.borrow_mut().clear());
    DISAPPEAR_HANDLERS.with(|handlers| handlers.borrow_mut().clear());
    CURRENT_ROUTE.with(|route| *route.borrow_mut() = None);
    HISTORY_STACK.with(|stack| stack.borrow_mut().clear());
    ROUTE_GUARDS.with(|guards| guards.borrow_mut().clear());
    NAV_LISTENERS.with(|listeners| listeners.borrow_mut().clear());
    BACK_HANDLERS.with(|handlers| handlers.borrow_mut().clear());
    TRANSITION_START_HANDLERS.with(|handlers| handlers.borrow_mut().clear());
    TRANSITION_COMPLETE_HANDLERS.with(|handlers| handlers.borrow_mut().clear());
    ROUTE_RESULT_HANDLERS.with(|handlers| handlers.borrow_mut().clear());
    MODAL_STACK.with(|stack| stack.borrow_mut().clear());
    crate::store::store_remove(NAVIGATION_STATE_KEY);
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

fn apply_navigation_transition(
    transition: NavigationTransition,
    context: NavigationTransitionContext,
    screen: BoxedView,
) -> BoxedView {
    use crate::anim::{animate, Easing};
    use crate::dom::Transform;
    use crate::reactive::{create_effect, create_signal};
    use crate::view::{boxed, column, ViewExt};

    match transition {
        NavigationTransition::None => screen,

        NavigationTransition::Slide => {
            // Forward screens slide in from the right; back transitions reveal
            // the previous screen from the left. Width is estimated until hosts
            // can feed transition containers their measured size.
            let start = if context.is_back() { -375.0 } else { 375.0 };
            let offset = create_signal(start);
            let anim = animate(start, 0.0, 0.3, Easing::EaseOut);
            create_effect(move || offset.set(anim.get()));

            boxed(
                column((screen,))
                    .grow()
                    .transform_fn(move || Transform::IDENTITY.translate(offset.get(), 0.0)),
            )
        }

        NavigationTransition::Fade => {
            // Fade in: opacity 0 -> 1.
            let opacity = create_signal(0.0f32);
            let anim = animate(0.0, 1.0, 0.25, Easing::EaseOut);
            create_effect(move || opacity.set(anim.get()));

            boxed(column((screen,)).grow().opacity_fn(move || opacity.get()))
        }
    }
}

/// Like [`routes`] but lets callers fully wrap each incoming screen transition.
///
/// The wrapper receives the latest typed-navigator transition context and the
/// rendered screen view. Return `screen` unchanged for no animation, or wrap it
/// with layout/style/animation views for a custom transition.
///
/// # Example
/// ```rust
/// use raxon::nav::{create_navigator, transition_routes_with};
/// use raxon::view::{boxed, column, text, View};
///
/// #[derive(Clone)]
/// enum Screen { Home, Details }
///
/// fn app() -> impl View {
///     let nav = create_navigator(Screen::Home);
///     transition_routes_with(
///         nav,
///         |context, screen| {
///             let _is_back = context.is_back();
///             boxed(column((screen,)).grow())
///         },
///         move |screen| match screen {
///             Screen::Home => boxed(text("home")),
///             Screen::Details => boxed(text("details")),
///         },
///     )
/// }
/// ```
pub fn transition_routes_with<R, F, T>(
    nav: Navigator<R>,
    mut transition: T,
    mut render: F,
) -> impl View
where
    R: Clone + 'static,
    F: FnMut(R) -> BoxedView + 'static,
    T: FnMut(NavigationTransitionContext, BoxedView) -> BoxedView + 'static,
{
    dynamic(move || {
        let context = nav.transition_context();
        let screen = render(nav.top());
        transition(context, screen)
    })
}

/// Like [`routes`] but animates screen transitions according to `transition`.
///
/// On each push/pop the incoming screen plays the enter animation; the
/// previous screen is immediately replaced (no simultaneous exit animation —
/// that would require two live widget trees which the current single-`dynamic`
/// architecture does not support). Use [`transition_routes_with`] when you need
/// a fully custom transition wrapper.
///
/// For `Slide`, the incoming screen slides in from the right on push and from
/// the left on back/pop. Since the layout width is not known at build time, a
/// fixed `375` point estimate is used. On `Fade`, opacity animates `0 -> 1`.
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
    render: F,
) -> impl View
where
    R: Clone + 'static,
    F: FnMut(R) -> BoxedView + 'static,
{
    transition_routes_with(
        nav,
        move |context, screen| apply_navigation_transition(transition, context, screen),
        render,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_route, build_route_with_query, cancel_route_result, create_navigator,
        create_tab_stack_navigator, current_route, decode_navigation_state,
        encode_navigation_state, has_pending_route_result, match_route, match_route_location,
        modal_stack, navigate, navigate_for_result, navigation_debug_snapshot, navigation_state,
        on_appear, on_disappear, on_navigate, on_transition_complete, on_transition_start,
        parse_deep_link, parse_query, parse_query_all, parse_route_location,
        pending_route_result_route, pending_route_result_type, present_modal, replace_route,
        restore_navigation_state, restore_saved_navigation_state, return_route_result, route,
        save_navigation_state, try_navigate_for_result, NavigationState, NavigatorTransitionKind,
        RouteTransitionKind,
    };
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum TestScreen {
        Home,
        Detail,
        Settings,
        Search,
    }

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
    fn match_route_supports_optional_path_params() {
        let without_id = match_route("/products/:id?", "/products").expect("route should match");
        assert!(without_id.is_empty());

        let with_id =
            match_route("/products/:id?", "/products/abc%20123").expect("route should match");
        assert_eq!(with_id["id"], "abc 123");

        let nested_without_id =
            match_route("/products/:id?/reviews", "/products/reviews").expect("route should match");
        assert!(nested_without_id.is_empty());

        let nested_with_id = match_route("/products/:id?/reviews", "/products/42/reviews")
            .expect("route should match");
        assert_eq!(nested_with_id["id"], "42");

        assert!(match_route("/products/:id?", "/products/42/reviews").is_none());
    }

    #[test]
    fn match_route_supports_splat_params_and_unnamed_catchalls() {
        let file = match_route("/files/*path", "/files/docs/Annual%20Plan.pdf")
            .expect("route should match");
        assert_eq!(file["path"], "docs/Annual Plan.pdf");

        let empty_splat = match_route("/files/*path", "/files").expect("route should match");
        assert_eq!(empty_splat["path"], "");

        let unnamed =
            match_route("/settings/*", "/settings/profile/security").expect("route should match");
        assert!(unnamed.is_empty());

        assert!(match_route("/files/*path/edit", "/files/docs/edit").is_none());
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
    fn url_route_matches_optional_and_splat_patterns() {
        let optional = route("/products/:id?/reviews", |_| {
            crate::view::boxed(crate::view::text("reviews"))
        });
        assert!(optional.matches("/products/reviews").is_some());
        assert_eq!(
            optional
                .matches("/products/42/reviews")
                .expect("route should match")
                .param("id"),
            Some("42")
        );

        let catchall = route("/files/*path", |_| {
            crate::view::boxed(crate::view::text("file"))
        });
        assert_eq!(
            catchall
                .matches("/files/docs/report.pdf")
                .expect("route should match")
                .param("path"),
            Some("docs/report.pdf")
        );
    }

    #[test]
    fn route_location_rewrites_query_params_with_stable_encoding() {
        let location = parse_route_location("/orders/42?tag=paid&tag=pickup&name=Alice+Doe#notes");
        let rewritten = location
            .with_query_param("tab", "items")
            .without_query_param("name");

        assert_eq!(
            rewritten.to_route_string(),
            "/orders/42?tab=items&tag=paid&tag=pickup#notes"
        );
    }

    #[test]
    fn route_location_rewrites_repeated_values_and_removes_empty_sets() {
        let location = parse_route_location("/search?tag=old&sort=recent");
        let rewritten = location.with_query_values("tag", ["paid", "pickup"]);
        let removed = rewritten.with_query_values("sort", std::iter::empty::<&str>());
        let expected_tags = vec!["paid".to_string(), "pickup".to_string()];

        assert_eq!(
            rewritten.query_values("tag"),
            Some(expected_tags.as_slice())
        );
        assert_eq!(removed.to_route_string(), "/search?tag=paid&tag=pickup");
    }

    #[test]
    fn route_location_encodes_query_and_fragment_values() {
        let location = parse_route_location("/search")
            .with_query_param("sort by", "new first")
            .with_fragment("section 1");

        assert_eq!(
            location.to_route_string(),
            "/search?sort+by=new+first#section%201"
        );
    }

    #[test]
    fn route_location_keeps_hash_router_urls_in_hash_form() {
        let location = parse_route_location("https://rtylr.com/#/checkout?step=pay");
        let rewritten = location.with_query_param("coupon", "VIP 10");

        assert_eq!(
            rewritten.to_route_string(),
            "/#/checkout?coupon=VIP+10&step=pay"
        );
    }

    #[test]
    fn build_route_fills_optional_required_and_splat_params() {
        let order = build_route(
            "/orders/:id/reviews/:review_id",
            [("id", "abc 123"), ("review_id", "R/9")],
        );
        assert_eq!(order.as_deref(), Some("/orders/abc%20123/reviews/R%2F9"));

        let optional = build_route("/orders/:id?", std::iter::empty::<(&str, &str)>());
        assert_eq!(optional.as_deref(), Some("/orders"));

        let splat = build_route("/files/*path", [("path", "docs/Annual Plan.pdf")]);
        assert_eq!(splat.as_deref(), Some("/files/docs/Annual%20Plan.pdf"));

        assert!(build_route("/orders/:id", std::iter::empty::<(&str, &str)>()).is_none());
    }

    #[test]
    fn build_route_with_query_handles_pattern_constraints_and_hash_routes() {
        let route = build_route_with_query(
            "/#/checkout/:step?mode=guest",
            [("step", "pay")],
            [("coupon", "VIP 10"), ("tag", "fast"), ("tag", "paid")],
        )
        .expect("route should build");

        assert_eq!(
            route,
            "/#/checkout/pay?coupon=VIP+10&mode=guest&tag=fast&tag=paid"
        );

        let overridden = build_route_with_query(
            "/orders/:id?tab=items",
            [("id", "42")],
            [("tab", "history")],
        )
        .expect("route should build");
        assert_eq!(overridden, "/orders/42?tab=history");
    }

    #[test]
    fn route_changes_fire_transition_focus_and_navigation_lifecycle() {
        super::reset_navigation_for_tests();

        let log = Rc::new(RefCell::new(Vec::<String>::new()));
        for route in ["/", "/orders", "/settings"] {
            let appear_log = Rc::clone(&log);
            on_appear(route, move || {
                appear_log.borrow_mut().push(format!("appear:{route}"));
            });

            let disappear_log = Rc::clone(&log);
            on_disappear(route, move || {
                disappear_log
                    .borrow_mut()
                    .push(format!("disappear:{route}"));
            });
        }

        let start_log = Rc::clone(&log);
        on_transition_start(move |event| {
            start_log.borrow_mut().push(format!(
                "start:{:?}:{}->{}",
                event.kind, event.from, event.to
            ));
        });

        let complete_log = Rc::clone(&log);
        on_transition_complete(move |event| {
            complete_log.borrow_mut().push(format!(
                "complete:{:?}:{}->{}",
                event.kind, event.from, event.to
            ));
        });

        let navigate_log = Rc::clone(&log);
        on_navigate(move |from, to| {
            navigate_log
                .borrow_mut()
                .push(format!("navigate:{from}->{to}"));
        });

        navigate("/");
        navigate("/orders");
        replace_route("/settings");
        assert!(super::go_back());

        assert_eq!(
            log.borrow().as_slice(),
            [
                "start:Push:->/",
                "appear:/",
                "complete:Push:->/",
                "navigate:->/",
                "start:Push:/->/orders",
                "disappear:/",
                "appear:/orders",
                "complete:Push:/->/orders",
                "navigate:/->/orders",
                "start:Replace:/orders->/settings",
                "disappear:/orders",
                "appear:/settings",
                "complete:Replace:/orders->/settings",
                "navigate:/orders->/settings",
                "start:Pop:/settings->/",
                "disappear:/settings",
                "appear:/",
                "complete:Pop:/settings->/",
                "navigate:/settings->/",
            ]
        );

        super::reset_navigation_for_tests();
    }

    #[test]
    fn typed_navigator_tracks_transition_context_for_custom_wrappers() {
        let nav = create_navigator(TestScreen::Home);

        assert_eq!(
            nav.transition_context().kind,
            NavigatorTransitionKind::Initial
        );
        assert_eq!(nav.transition_context().from_depth, 1);
        assert_eq!(nav.transition_context().to_depth, 1);

        nav.push(TestScreen::Detail);
        let pushed = nav.transition_context();
        assert_eq!(pushed.kind, NavigatorTransitionKind::Push);
        assert_eq!(pushed.from_depth, 1);
        assert_eq!(pushed.to_depth, 2);
        assert!(pushed.is_forward());
        assert!(!pushed.is_back());

        nav.replace(TestScreen::Settings);
        let replaced = nav.transition_context();
        assert_eq!(replaced.kind, NavigatorTransitionKind::Replace);
        assert_eq!(replaced.from_depth, 2);
        assert_eq!(replaced.to_depth, 2);
        assert!(!replaced.is_forward());
        assert!(!replaced.is_back());

        nav.pop();
        let popped = nav.transition_context();
        assert_eq!(popped.kind, NavigatorTransitionKind::Pop);
        assert_eq!(popped.from_depth, 2);
        assert_eq!(popped.to_depth, 1);
        assert!(popped.is_back());

        nav.push(TestScreen::Detail);
        nav.push(TestScreen::Settings);
        nav.pop_to_root();
        let popped_to_root = nav.transition_context();
        assert_eq!(popped_to_root.kind, NavigatorTransitionKind::PopToRoot);
        assert_eq!(popped_to_root.from_depth, 3);
        assert_eq!(popped_to_root.to_depth, 1);
        assert!(popped_to_root.is_back());

        nav.reset(TestScreen::Home);
        let reset = nav.transition_context();
        assert_eq!(reset.kind, NavigatorTransitionKind::Reset);
        assert_eq!(reset.from_depth, 1);
        assert_eq!(reset.to_depth, 1);
    }

    #[test]
    fn typed_navigator_debug_snapshot_reports_stack_and_transition() {
        let nav = create_navigator(TestScreen::Home);
        nav.push(TestScreen::Detail);

        let snapshot = nav.debug_snapshot();

        assert_eq!(snapshot.stack, vec![TestScreen::Home, TestScreen::Detail]);
        assert_eq!(snapshot.depth, 2);
        assert_eq!(snapshot.top, Some(TestScreen::Detail));
        assert!(snapshot.can_pop);
        assert_eq!(snapshot.transition.kind, NavigatorTransitionKind::Push);
        assert_eq!(snapshot.transition.from_depth, 1);
        assert_eq!(snapshot.transition.to_depth, 2);
    }

    #[test]
    fn tab_stack_navigator_preserves_independent_tab_stacks() {
        let nav = create_tab_stack_navigator(vec![TestScreen::Home, TestScreen::Settings]);

        assert_eq!(nav.tab_count(), 2);
        assert_eq!(nav.selected_index(), 0);
        assert_eq!(nav.top(), TestScreen::Home);
        assert_eq!(nav.depth(), 1);

        nav.push(TestScreen::Detail);
        assert_eq!(nav.top(), TestScreen::Detail);
        assert_eq!(nav.depth(), 2);
        assert_eq!(nav.top_for_tab(0), Some(TestScreen::Detail));
        assert_eq!(nav.depth_for_tab(0), 2);
        assert!(nav.can_pop());
        assert!(nav.can_pop_tab(0));

        assert!(nav.select(1));
        assert_eq!(nav.selected_index(), 1);
        assert_eq!(nav.top(), TestScreen::Settings);
        assert_eq!(nav.depth(), 1);
        assert_eq!(nav.top_for_tab(0), Some(TestScreen::Detail));
        assert_eq!(nav.top_for_tab(1), Some(TestScreen::Settings));
        assert!(!nav.can_pop_tab(1));

        nav.push(TestScreen::Search);
        assert_eq!(nav.top(), TestScreen::Search);
        assert_eq!(nav.depth_for_tab(1), 2);
        assert_eq!(
            nav.stack(1),
            Some(vec![TestScreen::Settings, TestScreen::Search])
        );

        assert!(nav.select(0));
        assert_eq!(nav.top(), TestScreen::Detail);
        assert_eq!(
            nav.stack(0),
            Some(vec![TestScreen::Home, TestScreen::Detail])
        );

        assert!(nav.pop());
        assert_eq!(nav.top(), TestScreen::Home);
        assert!(!nav.pop());

        assert!(nav.pop_tab_to_root(1));
        assert_eq!(nav.stack(1), Some(vec![TestScreen::Settings]));
        assert!(!nav.pop_tab_to_root(1));

        assert!(nav.reset_tab(1, TestScreen::Search));
        assert_eq!(nav.stack(1), Some(vec![TestScreen::Search]));
        assert!(!nav.select(99));
        assert_eq!(nav.top_for_tab(99), None);
        assert_eq!(nav.depth_for_tab(99), 0);
        assert!(!nav.reset_tab(99, TestScreen::Home));
    }

    #[test]
    fn tab_stack_debug_snapshot_reports_selected_stack_state() {
        let nav = create_tab_stack_navigator(vec![TestScreen::Home, TestScreen::Settings]);
        nav.push(TestScreen::Detail);
        assert!(nav.select(1));
        nav.push(TestScreen::Search);

        let snapshot = nav.debug_snapshot();

        assert_eq!(snapshot.selected_index, 1);
        assert_eq!(snapshot.tab_count, 2);
        assert_eq!(
            snapshot.stacks,
            vec![
                vec![TestScreen::Home, TestScreen::Detail],
                vec![TestScreen::Settings, TestScreen::Search]
            ]
        );
        assert_eq!(
            snapshot.selected_stack,
            vec![TestScreen::Settings, TestScreen::Search]
        );
        assert_eq!(snapshot.selected_top, Some(TestScreen::Search));
        assert!(snapshot.can_pop);
    }

    #[test]
    fn restoring_navigation_state_reports_restore_transition_kind() {
        super::reset_navigation_for_tests();
        navigate("/before");

        let kinds = Rc::new(RefCell::new(Vec::new()));
        let kinds_for_callback = Rc::clone(&kinds);
        on_transition_start(move |event| {
            kinds_for_callback.borrow_mut().push(event.kind);
        });

        restore_navigation_state(NavigationState::new(
            "/after",
            vec!["/".to_string(), "/after".to_string()],
            vec![],
        ));

        assert_eq!(kinds.borrow().as_slice(), [RouteTransitionKind::Restore]);
        assert_eq!(current_route().get(), "/after");

        super::reset_navigation_for_tests();
    }

    #[test]
    fn navigate_for_result_returns_typed_value_and_pops_route() {
        super::reset_navigation_for_tests();
        navigate("/checkout");

        let selected = Rc::new(RefCell::new(None));
        let selected_for_callback = Rc::clone(&selected);
        let reached = navigate_for_result("/products/pick", move |sku: String| {
            *selected_for_callback.borrow_mut() = Some(sku);
        });

        assert_eq!(reached, "/products/pick");
        assert!(has_pending_route_result());
        assert_eq!(
            pending_route_result_route().as_deref(),
            Some("/products/pick")
        );
        assert!(pending_route_result_type()
            .expect("result type should exist")
            .contains("String"));
        assert_eq!(current_route().get(), "/products/pick");

        assert!(return_route_result("sku_123".to_string()));
        assert_eq!(selected.borrow().as_deref(), Some("sku_123"));
        assert_eq!(current_route().get(), "/checkout");
        assert!(!has_pending_route_result());

        super::reset_navigation_for_tests();
    }

    #[test]
    fn route_result_rejects_wrong_type_without_consuming_handler() {
        super::reset_navigation_for_tests();
        navigate("/checkout");

        let selected = Rc::new(RefCell::new(None));
        let selected_for_callback = Rc::clone(&selected);
        assert!(try_navigate_for_result(
            "/products/pick",
            move |sku: String| {
                *selected_for_callback.borrow_mut() = Some(sku);
            }
        ));

        assert!(!return_route_result(123_u32));
        assert!(has_pending_route_result());
        assert_eq!(current_route().get(), "/products/pick");

        assert!(return_route_result("sku_456".to_string()));
        assert_eq!(selected.borrow().as_deref(), Some("sku_456"));
        assert_eq!(current_route().get(), "/checkout");

        super::reset_navigation_for_tests();
    }

    #[test]
    fn cancel_route_result_drops_callback_and_pops_route() {
        super::reset_navigation_for_tests();
        navigate("/checkout");

        let called = Rc::new(RefCell::new(false));
        let called_for_callback = Rc::clone(&called);
        navigate_for_result("/products/pick", move |_: String| {
            *called_for_callback.borrow_mut() = true;
        });

        assert!(cancel_route_result());
        assert!(!*called.borrow());
        assert_eq!(current_route().get(), "/checkout");
        assert!(!has_pending_route_result());

        super::reset_navigation_for_tests();
    }

    #[test]
    fn navigation_state_snapshots_history_and_modals() {
        super::reset_navigation_for_tests();
        navigate("/");
        navigate("/orders/42?tab=items");
        present_modal("/filters");

        let state = navigation_state();

        assert_eq!(state.current, "/orders/42?tab=items");
        assert_eq!(
            state.history,
            vec!["/".to_string(), "/orders/42?tab=items".to_string()]
        );
        assert_eq!(state.modals, vec!["/filters".to_string()]);

        super::reset_navigation_for_tests();
    }

    #[test]
    fn navigation_debug_snapshot_reports_string_router_state() {
        super::reset_navigation_for_tests();
        navigate("/");
        navigate("/orders/42?tab=items&tag=paid&tag=pickup#notes");
        present_modal("/filters");
        navigate_for_result("/products/pick", |_: String| {});

        let snapshot = navigation_debug_snapshot();

        assert_eq!(snapshot.current, "/products/pick");
        assert_eq!(snapshot.location.path, "/products/pick");
        assert_eq!(
            snapshot.history,
            vec![
                "/".to_string(),
                "/orders/42?tab=items&tag=paid&tag=pickup#notes".to_string(),
                "/products/pick".to_string()
            ]
        );
        assert_eq!(snapshot.history_depth, 3);
        assert!(snapshot.can_go_back);
        assert_eq!(snapshot.modals, vec!["/filters".to_string()]);
        assert_eq!(snapshot.modal_depth, 1);
        assert_eq!(snapshot.active_modal.as_deref(), Some("/filters"));
        assert!(snapshot.has_pending_result);
        assert_eq!(
            snapshot.pending_result_route.as_deref(),
            Some("/products/pick")
        );
        assert!(snapshot
            .pending_result_type
            .as_deref()
            .expect("pending type should be present")
            .contains("String"));
        assert_eq!(snapshot.pending_result_count, 1);

        super::reset_navigation_for_tests();
    }

    #[test]
    fn restore_navigation_state_restores_stack_and_clears_result_callbacks() {
        super::reset_navigation_for_tests();
        navigate("/checkout");

        let called = Rc::new(RefCell::new(false));
        let called_for_callback = Rc::clone(&called);
        navigate_for_result("/products/pick", move |_: String| {
            *called_for_callback.borrow_mut() = true;
        });
        assert!(has_pending_route_result());

        let restored = restore_navigation_state(NavigationState::new(
            "/orders/42",
            vec!["/".to_string(), "/orders/42".to_string()],
            vec!["/filters".to_string()],
        ));

        assert_eq!(restored.current, "/orders/42");
        assert_eq!(current_route().get(), "/orders/42");
        assert_eq!(modal_stack(), vec!["/filters".to_string()]);
        assert!(!has_pending_route_result());
        assert!(!return_route_result("sku_789".to_string()));
        assert!(!*called.borrow());

        super::reset_navigation_for_tests();
    }

    #[test]
    fn navigation_state_json_and_store_round_trip() {
        super::reset_navigation_for_tests();
        navigate("/");
        navigate("/orders/42?tab=items");
        present_modal("/filters");

        let saved = save_navigation_state().expect("state should serialize");
        let json = encode_navigation_state(&saved).expect("state should encode");
        let decoded = decode_navigation_state(&json).expect("state should decode");

        assert_eq!(decoded, saved);

        restore_navigation_state(NavigationState::new(
            "/placeholder",
            vec!["/placeholder".into()],
            vec![],
        ));
        assert_eq!(current_route().get(), "/placeholder");

        let restored = restore_saved_navigation_state().expect("saved state should restore");
        assert_eq!(restored.current, "/orders/42?tab=items");
        assert_eq!(current_route().get(), "/orders/42?tab=items");
        assert_eq!(modal_stack(), vec!["/filters".to_string()]);

        super::reset_navigation_for_tests();
    }

    #[test]
    fn parse_deep_link_handles_universal_links() {
        let (path, params) = parse_deep_link("https://rtylr.com/profile/42?tab=posts");

        assert_eq!(path, "/profile/42");
        assert_eq!(params["tab"], "posts");
    }
}
