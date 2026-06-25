//! Priority lanes, the frame-request flag, and the cross-thread [`Spawner`].
//!
//! This is the marshaling boundary that closes R1's last gap: a task spawned
//! from any thread is executed by the scheduler **on the UI thread** at a frame
//! boundary, so an async result computed off-thread can safely write signals.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

/// Scheduling priority for marshaled tasks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Run before normal work this frame (input responses, urgent updates).
    High,
    /// Default lane.
    Normal,
    /// Run only after the frame's visible work (prefetch, cleanup).
    Idle,
}

/// A boxed unit of work. `Send` so it can cross threads to reach the UI thread.
pub(crate) type Task = Box<dyn FnOnce() + Send>;

/// A cloneable, `Send` handle for requesting that a frame be scheduled.
///
/// Setting the flag is the cross-thread wakeup signal; a platform driver polls
/// [`is_requested`](FrameRequester::is_requested) (or is woken via the looper)
/// and calls `run_frame`.
#[derive(Clone, Debug)]
pub struct FrameRequester {
    flag: Arc<AtomicBool>,
}

impl FrameRequester {
    pub(crate) fn new() -> Self {
        FrameRequester {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Requests that a frame be run. Idempotent: many requests coalesce to one.
    pub fn request_frame(&self) {
        self.flag.store(true, Ordering::Release);
    }

    /// Whether a frame is currently pending.
    pub fn is_requested(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }

    /// Atomically reads and clears the flag (called at frame start).
    pub(crate) fn take(&self) -> bool {
        self.flag.swap(false, Ordering::AcqRel)
    }
}

/// A cloneable, `Send` handle for marshaling work onto the UI thread.
#[derive(Clone)]
pub struct Spawner {
    tx: Sender<(Priority, Task)>,
    requester: FrameRequester,
}

impl Spawner {
    pub(crate) fn new(tx: Sender<(Priority, Task)>, requester: FrameRequester) -> Self {
        Spawner { tx, requester }
    }

    /// Enqueues `task` at `priority`, to run on the UI thread at the next frame.
    /// Requesting a frame happens automatically.
    pub fn spawn(&self, priority: Priority, task: impl FnOnce() + Send + 'static) {
        // Send only fails if the scheduler (and its receiver) is gone; then the
        // app is shutting down and dropping the task is correct.
        if self.tx.send((priority, Box::new(task))).is_ok() {
            self.requester.request_frame();
        }
    }

    /// Enqueues a normal-priority task.
    pub fn spawn_normal(&self, task: impl FnOnce() + Send + 'static) {
        self.spawn(Priority::Normal, task);
    }
}
