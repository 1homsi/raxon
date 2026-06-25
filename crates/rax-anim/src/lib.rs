//! Animation for `rax`: tweened values that live in signals and are advanced by
//! the frame loop.
//!
//! [`animate`] returns a `Signal<f32>` that interpolates from a start to an end
//! value over a duration with an easing curve. Because it's a signal, any view
//! that reads it (e.g. inside a reactive `text` or a bound attribute) updates
//! automatically as the value changes — fine-grained, no tree diff.
//!
//! The runtime calls [`tick`] once per frame with the elapsed time; tests call
//! it directly with a fixed delta for determinism.
//!
//! ```
//! use rax_anim::{animate, tick, Easing};
//! use rax_reactive::create_root;
//!
//! let (a, scope) = create_root(|| animate(0.0, 100.0, 1.0, Easing::Linear));
//! assert_eq!(a.get(), 0.0);
//! tick(0.5); // halfway
//! assert!((a.get() - 50.0).abs() < 0.01);
//! tick(0.5); // done
//! assert_eq!(a.get(), 100.0);
//! scope.dispose();
//! ```

#![forbid(unsafe_code)]

use std::cell::RefCell;

use rax_reactive::Signal;

/// Easing curves applied to normalized time `t` in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Easing {
    /// Constant velocity.
    #[default]
    Linear,
    /// Accelerate from rest.
    EaseIn,
    /// Decelerate to rest.
    EaseOut,
    /// Accelerate then decelerate.
    EaseInOut,
}

impl Easing {
    /// Maps normalized time `t` (`0.0..=1.0`) through the curve.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t,
            Easing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
        }
    }
}

/// Parameters of a [`spring`] animation: a damped harmonic oscillator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    /// Restoring force toward the target (higher = snappier).
    pub stiffness: f32,
    /// Resistance that removes energy (higher = less bouncy).
    pub damping: f32,
    /// Mass of the body (higher = slower to accelerate).
    pub mass: f32,
}

impl Default for Spring {
    /// A gentle, slightly-bouncy default (à la react-spring).
    fn default() -> Self {
        Spring {
            stiffness: 170.0,
            damping: 26.0,
            mass: 1.0,
        }
    }
}

impl Spring {
    /// A stiff, snappy spring with no overshoot.
    pub const STIFF: Spring = Spring {
        stiffness: 210.0,
        damping: 30.0,
        mass: 1.0,
    };

    /// A loose, wobbly spring.
    pub const WOBBLY: Spring = Spring {
        stiffness: 180.0,
        damping: 12.0,
        mass: 1.0,
    };
}

struct Tween {
    signal: Signal<f32>,
    from: f32,
    to: f32,
    duration: f32,
    elapsed: f32,
    easing: Easing,
}

impl Tween {
    /// Advances by `dt` seconds; returns `true` when finished.
    fn advance(&mut self, dt: f32) -> bool {
        self.elapsed += dt;
        let t = if self.duration <= 0.0 {
            1.0
        } else {
            (self.elapsed / self.duration).min(1.0)
        };
        let value = self.from + (self.to - self.from) * self.easing.apply(t);
        self.signal.set(value);
        t >= 1.0
    }
}

struct SpringAnim {
    signal: Signal<f32>,
    target: f32,
    position: f32,
    velocity: f32,
    spring: Spring,
}

impl SpringAnim {
    /// Integrates the spring by `dt` seconds (sub-stepped for stability);
    /// returns `true` once it has settled at the target.
    fn advance(&mut self, dt: f32) -> bool {
        // Fixed 240 Hz sub-steps keep semi-implicit Euler stable for stiff
        // springs even at low frame rates.
        let steps = (dt * 240.0).ceil().max(1.0);
        let h = dt / steps;
        for _ in 0..(steps as u32) {
            let force =
                -self.spring.stiffness * (self.position - self.target) - self.spring.damping * self.velocity;
            let accel = force / self.spring.mass.max(0.0001);
            self.velocity += accel * h;
            self.position += self.velocity * h;
        }
        let settled = (self.position - self.target).abs() < 0.01 && self.velocity.abs() < 0.05;
        if settled {
            self.position = self.target;
            self.velocity = 0.0;
        }
        self.signal.set(self.position);
        settled
    }
}

enum Animation {
    Tween(Tween),
    Spring(SpringAnim),
}

impl Animation {
    fn advance(&mut self, dt: f32) -> bool {
        match self {
            Animation::Tween(t) => t.advance(dt),
            Animation::Spring(s) => s.advance(dt),
        }
    }
}

thread_local! {
    static ACTIVE: RefCell<Vec<Animation>> = const { RefCell::new(Vec::new()) };
}

/// Starts an animation from `from` to `to` over `duration` seconds with `easing`,
/// returning a signal that carries the animated value.
pub fn animate(from: f32, to: f32, duration: f32, easing: Easing) -> Signal<f32> {
    let signal = rax_reactive::create_signal(from);
    ACTIVE.with(|a| {
        a.borrow_mut().push(Animation::Tween(Tween {
            signal,
            from,
            to,
            duration,
            elapsed: 0.0,
            easing,
        }));
    });
    signal
}

/// Starts a spring animation from `from` to `to` with `spring` physics,
/// returning a signal that carries the animated value. Unlike [`animate`], a
/// spring has no fixed duration — it settles naturally and may overshoot.
///
/// ```
/// use rax_anim::{spring, tick, Spring};
/// use rax_reactive::create_root;
///
/// let (s, scope) = create_root(|| spring(0.0, 100.0, Spring::default()));
/// assert_eq!(s.get(), 0.0);
/// for _ in 0..600 { tick(1.0 / 60.0); } // run to rest
/// assert!((s.get() - 100.0).abs() < 0.1);
/// scope.dispose();
/// ```
pub fn spring(from: f32, to: f32, spring: Spring) -> Signal<f32> {
    let signal = rax_reactive::create_signal(from);
    ACTIVE.with(|a| {
        a.borrow_mut().push(Animation::Spring(SpringAnim {
            signal,
            target: to,
            position: from,
            velocity: 0.0,
            spring,
        }));
    });
    signal
}

/// Advances all active animations by `dt` seconds, dropping finished ones.
/// Called once per frame by the runtime. Returns the number still running.
pub fn tick(dt: f32) -> usize {
    // Take the list out so a `signal.set` (which runs effects that could, in
    // principle, start new animations) cannot alias the borrow.
    let mut tweens = ACTIVE.with(|a| std::mem::take(&mut *a.borrow_mut()));
    tweens.retain_mut(|tween| !tween.advance(dt));
    ACTIVE.with(|a| {
        let mut active = a.borrow_mut();
        // Prepend the still-running ones before any started during advance.
        tweens.append(&mut active);
        *active = tweens;
    });
    ACTIVE.with(|a| a.borrow().len())
}

/// Whether any animation is currently running (the driver can idle otherwise).
pub fn is_animating() -> bool {
    ACTIVE.with(|a| !a.borrow().is_empty())
}

#[cfg(test)]
mod tests;
