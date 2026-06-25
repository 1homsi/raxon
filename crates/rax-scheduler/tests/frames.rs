//! Frame loop: request coalescing, phase ordering, timing, callback removal.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rax_scheduler::*;

#[test]
fn needs_frame_coalesces_and_clears() {
    let mut s = Scheduler::new(ManualClock::new());
    assert!(!s.needs_frame());

    s.request_frame();
    s.request_frame();
    assert!(
        s.needs_frame(),
        "many requests coalesce to one pending frame"
    );

    s.run_frame();
    assert!(!s.needs_frame(), "running a frame clears the request");
}

#[test]
fn phases_run_in_fixed_order() {
    let order = Rc::new(RefCell::new(Vec::<&str>::new()));
    let mut s = Scheduler::new(ManualClock::new());

    // Register in deliberately scrambled order; execution order must be fixed.
    for (phase, name) in [
        (Phase::Commit, "commit"),
        (Phase::PreFrame, "pre"),
        (Phase::PostFrame, "post"),
        (Phase::Layout, "layout"),
        (Phase::Frame, "frame"),
    ] {
        let order = order.clone();
        s.on_phase(phase, move |_| order.borrow_mut().push(name));
    }

    s.run_frame();
    assert_eq!(
        *order.borrow(),
        vec!["pre", "frame", "layout", "commit", "post"]
    );
}

#[test]
fn frame_info_carries_ids_and_deltas() {
    let infos = Rc::new(RefCell::new(Vec::<FrameInfo>::new()));
    let mut s = Scheduler::new(ManualClock::new());
    {
        let infos = infos.clone();
        s.on_phase(Phase::Frame, move |i| infos.borrow_mut().push(*i));
    }

    s.clock().advance_millis(16);
    s.run_frame(); // frame 0
    s.clock().advance_millis(16);
    s.run_frame(); // frame 1

    let infos = infos.borrow();
    assert_eq!(infos[0].id, 0);
    assert_eq!(infos[0].now_nanos, 16_000_000);
    assert_eq!(infos[0].delta_nanos, 0, "first frame has no previous frame");
    assert_eq!(infos[1].id, 1);
    assert_eq!(infos[1].delta_nanos, 16_000_000);
    assert!((infos[1].delta_secs() - 0.016).abs() < 1e-6);
}

#[test]
fn removed_callback_stops_running() {
    let count = Rc::new(Cell::new(0));
    let mut s = Scheduler::new(ManualClock::new());
    let id = {
        let count = count.clone();
        s.on_phase(Phase::Frame, move |_| count.set(count.get() + 1))
    };

    s.run_frame();
    assert_eq!(count.get(), 1);

    s.remove(id);
    s.run_frame();
    assert_eq!(count.get(), 1, "removed callback no longer runs");
}
