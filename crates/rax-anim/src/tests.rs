use std::cell::RefCell;
use std::rc::Rc;

use rax_reactive::{create_effect, create_root};

use super::{animate, is_animating, spring, tick, Easing, Spring};

#[test]
fn linear_animation_interpolates_and_finishes() {
    let (a, scope) = create_root(|| animate(0.0, 100.0, 1.0, Easing::Linear));
    assert_eq!(a.get(), 0.0);
    tick(0.25);
    assert!((a.get() - 25.0).abs() < 0.01);
    tick(0.25);
    assert!((a.get() - 50.0).abs() < 0.01);
    tick(0.5);
    assert_eq!(a.get(), 100.0);
    assert!(!is_animating(), "finished animations are dropped");
    scope.dispose();
}

#[test]
fn easing_changes_the_curve() {
    let (a, scope) = create_root(|| animate(0.0, 1.0, 1.0, Easing::EaseIn));
    tick(0.5);
    // EaseIn at t=0.5 is 0.25, below the linear midpoint.
    assert!(a.get() < 0.3, "ease-in lags at the midpoint: {}", a.get());
    scope.dispose();
}

#[test]
fn animation_drives_a_reactive_reader() {
    let log: Rc<RefCell<Vec<f32>>> = Rc::new(RefCell::new(Vec::new()));
    let log2 = log.clone();
    let (_a, scope) = create_root(move || {
        let a = animate(0.0, 10.0, 1.0, Easing::Linear);
        create_effect(move || log2.borrow_mut().push(a.get()));
    });

    assert_eq!(log.borrow()[0], 0.0);
    tick(1.0);
    assert_eq!(
        *log.borrow().last().unwrap(),
        10.0,
        "reader saw the final value"
    );
    scope.dispose();
}

#[test]
fn zero_duration_jumps_to_end() {
    let (a, scope) = create_root(|| animate(5.0, 9.0, 0.0, Easing::Linear));
    tick(0.016);
    assert_eq!(a.get(), 9.0);
    scope.dispose();
}

#[test]
fn spring_settles_at_target_and_clears() {
    let (s, scope) = create_root(|| spring(0.0, 100.0, Spring::default()));
    assert_eq!(s.get(), 0.0);
    // Run for a few simulated seconds at 60fps.
    for _ in 0..600 {
        tick(1.0 / 60.0);
        if !is_animating() {
            break;
        }
    }
    assert!(!is_animating(), "spring finished");
    assert!((s.get() - 100.0).abs() < 0.1, "settled at target: {}", s.get());
    scope.dispose();
}

#[test]
fn wobbly_spring_overshoots_target() {
    let (s, scope) = create_root(|| spring(0.0, 100.0, Spring::WOBBLY));
    let mut max = 0.0_f32;
    for _ in 0..600 {
        tick(1.0 / 60.0);
        max = max.max(s.get());
        if !is_animating() {
            break;
        }
    }
    assert!(max > 100.0, "a wobbly spring overshoots (peak {max})");
    scope.dispose();
}
