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
