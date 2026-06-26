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
    /// Overshoot backward at start, then accelerate forward.
    EaseInBack,
    /// Overshoot past the target, then settle back.
    EaseOutBack,
    /// Overshoot on both ends (back in, back out).
    EaseInOutBack,
    /// Elastic snap at the beginning, oscillating before taking off.
    EaseInElastic,
    /// Elastic snap at the end, oscillating before settling.
    EaseOutElastic,
    /// Bounce at the end like a dropped ball.
    EaseOutBounce,
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
            Easing::EaseInBack => {
                // c1 = 1.70158 (standard overshoot constant)
                const C1: f32 = 1.701_58;
                const C3: f32 = C1 + 1.0;
                C3 * t * t * t - C1 * t * t
            }
            Easing::EaseOutBack => {
                const C1: f32 = 1.701_58;
                const C3: f32 = C1 + 1.0;
                1.0 + C3 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2)
            }
            Easing::EaseInOutBack => {
                const C1: f32 = 1.701_58;
                const C2: f32 = C1 * 1.525;
                if t < 0.5 {
                    ((2.0 * t).powi(2) * ((C2 + 1.0) * 2.0 * t - C2)) / 2.0
                } else {
                    ((2.0 * t - 2.0).powi(2) * ((C2 + 1.0) * (2.0 * t - 2.0) + C2) + 2.0) / 2.0
                }
            }
            Easing::EaseInElastic => {
                if t == 0.0 {
                    return 0.0;
                }
                if t == 1.0 {
                    return 1.0;
                }
                // C4 sets the oscillation period (2π/3 ≈ one full bounce).
                const C4: f32 = std::f32::consts::TAU / 3.0;
                let pow = 10.0 * t - 10.0;
                -(2.0_f32.powf(pow)) * ((pow - C4) / C4).sin()
            }
            Easing::EaseOutElastic => {
                if t == 0.0 {
                    return 0.0;
                }
                if t == 1.0 {
                    return 1.0;
                }
                const C4: f32 = std::f32::consts::TAU / 3.0;
                let pow = -10.0 * t;
                2.0_f32.powf(pow) * ((pow - C4) / C4).sin() + 1.0
            }
            Easing::EaseOutBounce => {
                const N1: f32 = 7.5625;
                const D1: f32 = 2.75;
                if t < 1.0 / D1 {
                    N1 * t * t
                } else if t < 2.0 / D1 {
                    let t = t - 1.5 / D1;
                    N1 * t * t + 0.75
                } else if t < 2.5 / D1 {
                    let t = t - 2.25 / D1;
                    N1 * t * t + 0.9375
                } else {
                    let t = t - 2.625 / D1;
                    N1 * t * t + 0.984_375
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

    /// A gentle, natural-feeling spring — smooth deceleration with no bounce.
    pub const GENTLE: Spring = Spring {
        stiffness: 100.0,
        damping: 15.0,
        mass: 1.0,
    };

    /// A bouncy, playful spring with noticeable overshoot.
    pub const BOUNCY: Spring = Spring {
        stiffness: 300.0,
        damping: 10.0,
        mass: 1.0,
    };

    /// A snappy spring that arrives fast with minimal oscillation.
    pub const SNAPPY: Spring = Spring {
        stiffness: 500.0,
        damping: 30.0,
        mass: 1.0,
    };

    /// A slow, dramatic spring for emphasis.
    pub const SLOW: Spring = Spring {
        stiffness: 50.0,
        damping: 20.0,
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

struct Decay {
    signal: Signal<f32>,
    position: f32,
    velocity: f32,
    /// Per-millisecond velocity retention (e.g. `0.998`).
    deceleration: f32,
}

impl Decay {
    /// Integrates velocity decay by `dt` seconds; returns `true` once it stops.
    fn advance(&mut self, dt: f32) -> bool {
        let steps = (dt * 240.0).ceil().max(1.0);
        let h = dt / steps;
        for _ in 0..(steps as u32) {
            self.position += self.velocity * h;
            self.velocity *= self.deceleration.powf(h * 1000.0);
        }
        self.signal.set(self.position);
        self.velocity.abs() < 1.0
    }
}

enum Animation {
    Tween(Tween),
    Spring(SpringAnim),
    Decay(Decay),
}

impl Animation {
    fn advance(&mut self, dt: f32) -> bool {
        match self {
            Animation::Tween(t) => t.advance(dt),
            Animation::Spring(s) => s.advance(dt),
            Animation::Decay(d) => d.advance(dt),
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

/// Starts a decay (fling) animation from `from` with an initial `velocity`
/// (units per second), coasting to a stop. `deceleration` is the per-millisecond
/// velocity retention (`0.998` ≈ a normal scroll fling; smaller stops sooner).
/// Returns a signal carrying the position.
///
/// ```
/// use rax_anim::{decay, tick};
/// use rax_reactive::create_root;
///
/// let (p, scope) = create_root(|| decay(0.0, 1200.0, 0.998));
/// for _ in 0..600 { tick(1.0 / 60.0); }
/// assert!(p.get() > 0.0); // coasted forward then stopped
/// scope.dispose();
/// ```
pub fn decay(from: f32, velocity: f32, deceleration: f32) -> Signal<f32> {
    let signal = rax_reactive::create_signal(from);
    ACTIVE.with(|a| {
        a.borrow_mut().push(Animation::Decay(Decay {
            signal,
            position: from,
            velocity,
            deceleration,
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

// ── Composition helpers ───────────────────────────────────────────────────────

/// Runs two animated values in parallel.
///
/// Because [`animate`] / [`spring`] / [`decay`] all start immediately upon
/// call and return independent [`Signal`]s, running them in parallel is the
/// default behaviour. This function is a *documentation helper*: it takes two
/// already-started signals and returns them as a tuple, making intent explicit
/// in code.
///
/// # Example
/// ```rust
/// use rax_anim::{animate, parallel, Easing};
/// use rax_reactive::create_root;
///
/// let ((x, y), scope) = create_root(|| {
///     parallel(
///         animate(0.0, 100.0, 0.3, Easing::EaseOut),
///         animate(0.0,  50.0, 0.3, Easing::EaseOut),
///     )
/// });
/// scope.dispose();
/// ```
pub fn parallel<A: 'static, B: 'static>(a: Signal<A>, b: Signal<B>) -> (Signal<A>, Signal<B>) {
    (a, b)
}

/// Runs a second animation after a first one completes.
///
/// Watches `first` until it reaches `to` (within 0.01 units), then calls
/// `second` once. The check runs inside a reactive effect so it fires
/// automatically on every signal update.
///
/// # Limitations
/// This is a best-effort helper: it triggers `second` the first time `first`
/// stabilises near `to`. If the animated value overshoots (e.g. a spring) or
/// never exactly reaches `to`, the threshold (`0.01`) may need tuning. For
/// frame-perfect sequencing, use a timer future via `rax_async::spawn_local`.
///
/// # Example
/// ```rust
/// use rax_anim::{animate, sequence, tick, Easing};
/// use rax_reactive::{create_root, create_signal};
///
/// let (second_started, scope) = create_root(|| {
///     let flag = create_signal(false);
///     let first = animate(0.0, 100.0, 0.5, Easing::Linear);
///     sequence(first, 100.0, move || flag.set(true));
///     flag
/// });
/// for _ in 0..60 { tick(1.0 / 60.0); }
/// assert!(second_started.get());
/// scope.dispose();
/// ```
pub fn sequence(first: Signal<f32>, to: f32, second: impl FnOnce() + 'static) {
    use std::cell::Cell;
    let fired = std::rc::Rc::new(Cell::new(false));
    let second = std::cell::RefCell::new(Some(second));
    rax_reactive::create_effect(move || {
        if fired.get() {
            return;
        }
        if (first.get() - to).abs() < 0.01 {
            fired.set(true);
            if let Some(f) = second.borrow_mut().take() {
                f();
            }
        }
    });
}

/// Staggers `n` animations, calling `make_anim(i)` for each index.
///
/// **Timer support pending.** True staggering (starting animation `i` only
/// after `delay_ms * i` milliseconds) requires wall-clock timer callbacks,
/// which are not yet available in `rax_anim`. This function currently calls
/// `make_anim` for every index **immediately**, so all animations start at the
/// same time. The `delay_ms` parameter is accepted but ignored.
///
/// Once `rax_async` timer primitives are stable, this will be updated to
/// honour the delay without breaking callers.
///
/// # Example
/// ```rust
/// use rax_anim::{stagger, animate, Easing};
/// use rax_reactive::create_root;
///
/// let (signals, scope) = create_root(|| {
///     stagger(3, 50, |i| animate(0.0, 100.0, 0.3, Easing::EaseOut))
/// });
/// scope.dispose();
/// ```
pub fn stagger<F>(n: usize, _delay_ms: u32, mut make_anim: F) -> Vec<Signal<f32>>
where
    F: FnMut(usize) -> Signal<f32>,
{
    (0..n).map(|i| make_anim(i)).collect()
}

/// Creates an oscillating animation that bounces between `from` and `to`.
///
/// The returned [`Signal`] carries the current animated position. Internally
/// the function uses a direction flag and nested reactive effects:
///
/// 1. An outer effect watches `dir` (true = forward, false = reverse) and
///    starts a new [`animate`] leg each time the direction flips.
/// 2. An inner effect copies the current leg's value into the returned signal
///    and flips `dir` when the leg nears its target, which re-triggers step 1.
///
/// This approach works entirely within the existing reactive/animation
/// scheduler — no timers needed. Accuracy depends on frame rate; the
/// completion threshold is `0.5` units.
///
/// # Example
/// ```rust
/// use rax_anim::{oscillate, tick, Easing};
/// use rax_reactive::create_root;
///
/// let (v, scope) = create_root(|| oscillate(0.0, 100.0, 1.0, Easing::Linear));
/// tick(0.6);          // approaching 60
/// let mid = v.get();
/// assert!(mid > 0.0 && mid < 100.0, "mid={mid}");
/// scope.dispose();
/// ```
pub fn oscillate(from: f32, to: f32, duration: f32, easing: Easing) -> Signal<f32> {
    let v = rax_reactive::create_signal(from);
    let dir = rax_reactive::create_signal(true); // true = forward

    rax_reactive::create_effect(move || {
        let forward = dir.get();
        let (a, b) = if forward { (from, to) } else { (to, from) };
        let anim_val = animate(a, b, duration, easing);
        // Inner effect: mirror value and flip direction when the leg ends.
        rax_reactive::create_effect(move || {
            let cur = anim_val.get();
            v.set(cur);
            let target = if forward { to } else { from };
            if (cur - target).abs() < 0.5 {
                dir.update(|d| *d = !*d);
            }
        });
    });
    v
}

/// Returns a signal that starts animating only after `start_trigger` becomes
/// `true`.
///
/// Until the trigger fires the signal holds `from`. Once `start_trigger` is
/// `true`, [`animate`] is started and the returned signal mirrors its value.
///
/// # Limitations
/// For precise wall-clock delays, combine a timer future from
/// `rax_async::spawn_local` with a simple `Signal<bool>` trigger instead.
///
/// # Example
/// ```rust
/// use rax_anim::{delayed, tick, Easing};
/// use rax_reactive::{create_root, create_signal};
///
/// let (val, scope) = create_root(|| {
///     let trigger = create_signal(false);
///     let v = delayed(trigger, 0.0, 100.0, 0.5, Easing::Linear);
///     trigger.set(true);   // fire the trigger
///     v
/// });
/// tick(0.5);
/// assert!((val.get() - 100.0).abs() < 0.5);
/// scope.dispose();
/// ```
pub fn delayed(
    start_trigger: Signal<bool>,
    from: f32,
    to: f32,
    duration: f32,
    easing: Easing,
) -> Signal<f32> {
    let value = rax_reactive::create_signal(from);
    rax_reactive::create_effect(move || {
        if start_trigger.get() {
            let anim = animate(from, to, duration, easing);
            rax_reactive::create_effect(move || value.set(anim.get()));
        }
    });
    value
}

// ---------------------------------------------------------------------------
// Off-main-thread animation infrastructure
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// An animation value computed on the dedicated animation thread.
///
/// Write with [`animate_offthread`]; read with [`OffThreadValue::get`] from
/// any thread (typically the main thread during layout/rendering).
///
/// Internally the value is stored as the IEEE 754 bit pattern of an `f32`
/// inside an `AtomicU64`, so reads and writes are wait-free.
#[derive(Clone)]
pub struct OffThreadValue {
    inner: Arc<AtomicU64>,
}

impl OffThreadValue {
    /// Creates a new value initialized to `initial`.
    pub fn new(initial: f32) -> Self {
        OffThreadValue {
            inner: Arc::new(AtomicU64::new(initial.to_bits() as u64)),
        }
    }

    /// Reads the current animated value (safe to call from any thread).
    pub fn get(&self) -> f32 {
        f32::from_bits(self.inner.load(Ordering::Relaxed) as u32)
    }

    /// Writes the animated value (called by the animation thread).
    pub fn set(&self, v: f32) {
        self.inner.store(v.to_bits() as u64, Ordering::Relaxed);
    }
}

trait OffThreadAnimatable: Send + Sync {
    /// Advances the animation by one ~120 Hz tick (~8.333 ms). Returns `true`
    /// when the animation has finished and should be removed.
    fn tick(&mut self) -> bool;
}

/// Global list of active off-thread animations. Guarded by a `Mutex` so the
/// animation thread and the main thread can both access it safely.
fn offthread_animations() -> &'static Mutex<Vec<Box<dyn OffThreadAnimatable>>> {
    static CELL: OnceLock<Mutex<Vec<Box<dyn OffThreadAnimatable>>>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(Vec::new()))
}

struct OffThreadTimingAnim {
    value: OffThreadValue,
    from: f32,
    target: f32,
    elapsed_ms: f32,
    duration_ms: f32,
    easing: Easing,
}

impl OffThreadAnimatable for OffThreadTimingAnim {
    fn tick(&mut self) -> bool {
        // Each tick is ~8.333 ms at 120 Hz.
        self.elapsed_ms += 8.333;
        let t = (self.elapsed_ms / self.duration_ms).min(1.0);
        let eased = self.easing.apply(t);
        self.value.set(self.from + (self.target - self.from) * eased);
        self.elapsed_ms >= self.duration_ms
    }
}

/// Enqueue an animation of `value` from its current value to `target` over
/// `duration_ms` milliseconds on the off-main-thread animation queue.
///
/// The animation is driven by the thread started by [`start_animation_thread`].
/// Call `start_animation_thread` once at app startup before calling this.
///
/// # Example
/// ```
/// use rax_anim::{OffThreadValue, animate_offthread, Easing};
///
/// let v = OffThreadValue::new(0.0);
/// animate_offthread(&v, 1.0, 300, Easing::EaseOut);
/// assert!(v.get() <= 1.0);
/// ```
pub fn animate_offthread(value: &OffThreadValue, target: f32, duration_ms: u32, easing: Easing) {
    let anim = OffThreadTimingAnim {
        value: value.clone(),
        from: value.get(),
        target,
        elapsed_ms: 0.0,
        duration_ms: duration_ms as f32,
        easing,
    };
    if let Ok(mut guard) = offthread_animations().lock() {
        guard.push(Box::new(anim));
    }
}

/// Spawns the dedicated animation background thread running at ~120 Hz.
///
/// Call **once** at app startup (before the first frame). The thread runs
/// indefinitely — it evaluates all animations registered via
/// [`animate_offthread`] and writes results into their [`OffThreadValue`]s.
/// The main thread reads those values during layout/rendering without any
/// locking overhead (atomic load).
///
/// # Example
/// ```no_run
/// use rax_anim::start_animation_thread;
///
/// let _handle = start_animation_thread();
/// ```
pub fn start_animation_thread() -> std::thread::JoinHandle<()> {
    std::thread::spawn(|| {
        let frame_duration = std::time::Duration::from_micros(8_333); // ~120 Hz
        loop {
            let start = std::time::Instant::now();

            // Advance and prune all registered off-thread animations.
            if let Ok(mut guard) = offthread_animations().lock() {
                guard.retain_mut(|anim| !anim.tick());
            }

            let elapsed = start.elapsed();
            if elapsed < frame_duration {
                std::thread::sleep(frame_duration - elapsed);
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Accessibility: reduced-motion support
// ---------------------------------------------------------------------------

thread_local! {
    static REDUCED_MOTION: std::cell::Cell<Option<rax_reactive::Signal<bool>>> =
        const { std::cell::Cell::new(None) };
}

/// Returns a reactive [`Signal<bool>`] that is `true` when the user has
/// enabled the "Reduce Motion" accessibility setting.
///
/// The signal starts as `false` and is updated by the platform backend via
/// [`set_reduced_motion`]. Because it's a signal, any view that reads it will
/// re-render automatically when the setting changes (e.g. if the user toggles
/// it in Settings without restarting the app).
///
/// Use together with [`animate_unless_reduced`] to skip animations when the
/// user prefers reduced motion.
///
/// # Example
/// ```rust
/// use rax_anim::use_reduced_motion;
/// use rax_reactive::create_root;
///
/// let (reduced, scope) = create_root(|| use_reduced_motion());
/// assert!(!reduced.get());
/// scope.dispose();
/// ```
pub fn use_reduced_motion() -> rax_reactive::Signal<bool> {
    if let Some(s) = REDUCED_MOTION.with(|c| c.get()) {
        return s;
    }
    let s = rax_reactive::create_signal(false);
    REDUCED_MOTION.with(|c| c.set(Some(s)));
    s
}

/// Updates the reduced-motion signal.
///
/// Call this from the platform backend whenever the OS accessibility setting
/// changes. On iOS this corresponds to `UIAccessibility.isReduceMotionEnabled`;
/// on macOS to `NSWorkspace.shared.accessibilityDisplayShouldReduceMotion`.
///
/// # Example (platform backend)
/// ```rust
/// use rax_anim::set_reduced_motion;
///
/// fn on_accessibility_changed(reduced: bool) {
///     set_reduced_motion(reduced);
/// }
/// ```
pub fn set_reduced_motion(enabled: bool) {
    if let Some(s) = REDUCED_MOTION.with(|c| c.get()) {
        s.set(enabled);
    }
}

/// Runs an animation to `target` unless the user has enabled reduced motion,
/// in which case the value is set immediately without any interpolation.
///
/// This is the recommended helper for all user-visible animations: wrapping
/// every `animate` call with `animate_unless_reduced` makes the app
/// automatically honour the accessibility preference with no extra logic at
/// the call site.
///
/// # Parameters
/// - `signal` — the signal whose value should change (typically created with
///   [`animate`] on a previous frame, or with `rax_reactive::create_signal`).
/// - `target` — the destination value.
/// - `duration` — animation duration in seconds (passed to [`animate`]).
/// - `easing` — the easing curve (passed to [`animate`]).
///
/// # Example
/// ```rust
/// use rax_anim::{animate_unless_reduced, use_reduced_motion, Easing};
/// use rax_reactive::{create_root, create_signal};
///
/// let (sig, scope) = create_root(|| {
///     let sig = create_signal(0.0f32);
///     animate_unless_reduced(sig, 100.0, 0.3, Easing::EaseOut);
///     sig
/// });
/// // With reduced motion disabled, the signal starts at 0 and animates.
/// assert!(sig.get() <= 100.0);
/// scope.dispose();
/// ```
pub fn animate_unless_reduced(
    signal: rax_reactive::Signal<f32>,
    target: f32,
    duration: f32,
    easing: Easing,
) {
    if use_reduced_motion().get() {
        signal.set(target);
    } else {
        let anim = animate(signal.get(), target, duration, easing);
        rax_reactive::create_effect(move || signal.set(anim.get()));
    }
}

// ---------------------------------------------------------------------------
// Interpolation helpers
// ---------------------------------------------------------------------------

/// Linear interpolation between `a` and `b`, clamped to `t` in `0.0..=1.0`.
///
/// # Example
/// ```
/// use rax_anim::lerp;
/// assert!((lerp(0.0, 100.0, 0.5) - 50.0).abs() < 0.001);
/// assert_eq!(lerp(0.0, 100.0, 0.0), 0.0);
/// assert_eq!(lerp(0.0, 100.0, 1.0), 100.0);
/// ```
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Smooth-step interpolation (3t²−2t³), clamped to `t` in `0.0..=1.0`.
///
/// Produces a gentle S-curve with zero first derivatives at `t=0` and `t=1`,
/// making transitions look more natural than a raw linear blend.
///
/// # Example
/// ```
/// use rax_anim::smooth_step;
/// assert_eq!(smooth_step(0.0), 0.0);
/// assert_eq!(smooth_step(1.0), 1.0);
/// assert!((smooth_step(0.5) - 0.5).abs() < 0.001); // midpoint is symmetric
/// ```
pub fn smooth_step(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Maps `value` from `[in_min, in_max]` to `[out_min, out_max]`.
///
/// Values outside the input range are extrapolated linearly (no clamping). To
/// clamp the output, call `.clamp(out_min, out_max)` on the result, or pass
/// the output through [`lerp`].
///
/// # Example
/// ```
/// use rax_anim::remap;
/// // Map scroll offset 0–200 → opacity 0.0–1.0
/// assert!((remap(100.0, 0.0, 200.0, 0.0, 1.0) - 0.5).abs() < 0.001);
/// ```
pub fn remap(value: f32, in_min: f32, in_max: f32, out_min: f32, out_max: f32) -> f32 {
    let t = (value - in_min) / (in_max - in_min);
    lerp(out_min, out_max, t)
}

// ---------------------------------------------------------------------------
// Rubber-band / overscroll
// ---------------------------------------------------------------------------

/// Applies iOS-style rubber-band resistance to a value outside `[min, max]`.
///
/// Inside the range the value is returned unchanged. Outside, resistance grows
/// with distance so the content slows but never fully stops — matching the feel
/// of `UIScrollView` overscroll.
///
/// Formula: `rubber = excess × (1 − 1 / (excess / range + 1))`
///
/// # Example
/// ```
/// use rax_anim::rubber_band;
/// // Inside range — no effect.
/// assert_eq!(rubber_band(50.0, 0.0, 100.0), 50.0);
/// // Below min — value is pulled back toward min with resistance.
/// let rb = rubber_band(-20.0, 0.0, 100.0);
/// assert!(rb > -20.0 && rb < 0.0);
/// // Above max — value is pushed back toward max with resistance.
/// let rb = rubber_band(120.0, 0.0, 100.0);
/// assert!(rb > 100.0 && rb < 120.0);
/// ```
pub fn rubber_band(value: f32, min: f32, max: f32) -> f32 {
    let range = (max - min).max(f32::EPSILON); // avoid division by zero
    if value < min {
        let excess = min - value;
        min - excess * (1.0 - 1.0 / (excess / range + 1.0))
    } else if value > max {
        let excess = value - max;
        max + excess * (1.0 - 1.0 / (excess / range + 1.0))
    } else {
        value
    }
}

// ---------------------------------------------------------------------------
// Transition helper
// ---------------------------------------------------------------------------

/// Returns a `Signal<f32>` (opacity, `0.0..=1.0`) that fades out then back in
/// whenever `key_fn` produces a new value.
///
/// The transition is split equally: the first half of `duration_ms` fades
/// **out** (`1→0`, `EaseOut`) and the second half fades **in** (`0→1`,
/// `EaseIn`). When the key is first observed no transition plays — the signal
/// starts at `1.0`.
///
/// Use alongside [`crate::animate`] and `dynamic` from `rax-view` to crossfade
/// between screens or content regions:
///
/// # Example
/// ```no_run
/// use rax_anim::use_transition;
/// use rax_reactive::{create_root, create_signal};
///
/// let (fade, scope) = create_root(|| {
///     let current_screen = create_signal(0u32);
///     use_transition(move || current_screen.get(), 300)
///     // Then in the view layer: dynamic(move || my_view().opacity(fade.get()))
/// });
/// scope.dispose();
/// ```
pub fn use_transition<K>(key_fn: impl Fn() -> K + 'static, duration_ms: u64) -> Signal<f32>
where
    K: Clone + PartialEq + 'static,
{
    let opacity = rax_reactive::create_signal(1.0_f32);
    let last_key: Signal<Option<K>> = rax_reactive::create_signal(None);

    rax_reactive::create_effect(move || {
        let k = key_fn();
        let prev = last_key.get();
        if prev.as_ref() == Some(&k) {
            return;
        }
        last_key.set(Some(k));

        let half_secs = (duration_ms / 2) as f32 / 1000.0;

        // Kick off the fade-out leg. Once it settles at 0.0 the `sequence`
        // callback starts the fade-in leg.
        let fade_out = animate(opacity.get(), 0.0, half_secs, Easing::EaseOut);

        // Mirror the fade-out into `opacity` while it runs, and chain fade-in.
        rax_reactive::create_effect(move || {
            let v = fade_out.get();
            opacity.set(v);
        });

        sequence(fade_out, 0.0, move || {
            let fade_in = animate(0.0, 1.0, half_secs, Easing::EaseIn);
            rax_reactive::create_effect(move || {
                opacity.set(fade_in.get());
            });
        });
    });

    opacity
}

// ---------------------------------------------------------------------------
// Keyframe animations
// ---------------------------------------------------------------------------

/// A single keyframe: a value at a normalized position in the timeline.
pub struct Keyframe<T: Clone> {
    /// Position in `0.0..=1.0`.
    pub progress: f32,
    /// Value at this keyframe.
    pub value: T,
    /// Easing from this keyframe to the next.
    pub easing: Easing,
}

/// Animate through a sequence of keyframes.
///
/// `clock` is a `Signal<f32>` from `0.0` to `1.0`. The returned `Signal<f32>`
/// holds the interpolated value between surrounding keyframes.
///
/// Only `f32` keyframes are supported via this overload. Use [`lerp`] directly
/// for custom types.
///
/// # Panics
/// Panics if `frames` is empty.
pub fn keyframes(clock: rax_reactive::Signal<f32>, frames: Vec<Keyframe<f32>>) -> rax_reactive::Signal<f32> {
    assert!(!frames.is_empty(), "keyframes requires at least one frame");
    let out = rax_reactive::create_signal(frames[0].value);
    rax_reactive::create_effect(move || {
        let t = clock.get().clamp(0.0, 1.0);
        let n = frames.len();
        let mut prev = &frames[0];
        let mut next = &frames[n - 1];
        for i in 0..n.saturating_sub(1) {
            if frames[i].progress <= t && t <= frames[i + 1].progress {
                prev = &frames[i];
                next = &frames[i + 1];
                break;
            }
        }
        let span = next.progress - prev.progress;
        let local_t = if span.abs() < f32::EPSILON { 1.0 } else { (t - prev.progress) / span };
        let eased = prev.easing.apply(local_t);
        out.set(lerp(prev.value, next.value, eased));
    });
    out
}

// ---------------------------------------------------------------------------
// Animation frame clock (frame-counter driven)
// ---------------------------------------------------------------------------

thread_local! {
    static ANIM_FRAME: std::cell::Cell<u64> = std::cell::Cell::new(0);
}

/// Advance the internal animation frame counter. Call once per display frame.
pub fn tick_animation_frame() {
    ANIM_FRAME.with(|f| f.set(f.get() + 1));
}

/// Returns the current animation frame count.
pub fn animation_frame() -> u64 {
    ANIM_FRAME.with(|f| f.get())
}

/// A `Signal<f32>` that cycles `0.0 → 1.0` every `period_frames` frames.
pub fn use_looping_clock(period_frames: u64) -> rax_reactive::Signal<f32> {
    let sig = rax_reactive::create_signal(0.0f32);
    rax_reactive::create_effect(move || {
        let frame = animation_frame();
        let v = if period_frames == 0 { 0.0 } else { (frame % period_frames) as f32 / period_frames as f32 };
        sig.set(v);
    });
    sig
}

#[cfg(test)]
mod tests;
