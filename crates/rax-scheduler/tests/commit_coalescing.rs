//! The R2 thesis: many state changes between frames coalesce into exactly one
//! commit per frame. This models how `rax-dom` bindings will defer mutation
//! emission to the scheduler's `Commit` phase instead of emitting eagerly.

use std::cell::RefCell;
use std::rc::Rc;

use rax_scheduler::*;

#[test]
fn writes_between_frames_coalesce_into_one_commit() {
    // `pending` stands in for the per-frame command buffer; `committed` records
    // each batch handed to the backend.
    let pending = Rc::new(RefCell::new(Vec::<i32>::new()));
    let committed = Rc::new(RefCell::new(Vec::<Vec<i32>>::new()));

    let mut s = Scheduler::new(ManualClock::new());
    {
        let pending = pending.clone();
        let committed = committed.clone();
        s.on_phase(Phase::Commit, move |_| {
            let batch = std::mem::take(&mut *pending.borrow_mut());
            if !batch.is_empty() {
                committed.borrow_mut().push(batch);
            }
        });
    }

    // Three "writes" before any frame: each buffers a mutation and asks for a frame.
    pending.borrow_mut().push(1);
    s.request_frame();
    pending.borrow_mut().push(2);
    s.request_frame();
    pending.borrow_mut().push(3);
    s.request_frame();

    assert!(s.needs_frame());
    s.run_frame();

    assert_eq!(
        *committed.borrow(),
        vec![vec![1, 2, 3]],
        "one coalesced commit, not three"
    );
    assert!(!s.needs_frame());

    // A frame with no pending writes commits nothing.
    s.run_frame();
    assert_eq!(
        *committed.borrow(),
        vec![vec![1, 2, 3]],
        "empty frame produces no commit"
    );
}
