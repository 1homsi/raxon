//! The scheduler: owns the frame loop, the per-phase callback table, and the
//! priority task queue.
//!
//! It does **not** drive itself. A platform driver (or a test) calls
//! [`run_frame`](Scheduler::run_frame) when a frame is due — typically when
//! [`needs_frame`](Scheduler::needs_frame) is true, on the platform's vsync.

use std::sync::mpsc::{channel, Receiver, Sender};

use crate::clock::Clock;
use crate::frame::{FrameId, FrameInfo, Phase, PHASE_COUNT};
use crate::task::{FrameRequester, Priority, Spawner, Task};

/// Identifies a registered frame callback, for later removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallbackId(u64);

struct Entry {
    id: CallbackId,
    callback: Box<dyn FnMut(&FrameInfo)>,
}

/// The frame scheduler. Generic over a [`Clock`] so frame timing is
/// deterministic in tests (`ManualClock`) and real on device (`SystemClock`).
pub struct Scheduler<C: Clock> {
    clock: C,
    frame_id: FrameId,
    last_nanos: Option<u64>,
    requester: FrameRequester,
    task_tx: Sender<(Priority, Task)>,
    task_rx: Receiver<(Priority, Task)>,
    idle_queue: Vec<Task>,
    callbacks: [Vec<Entry>; PHASE_COUNT],
    next_callback: u64,
}

impl<C: Clock> Scheduler<C> {
    /// Creates a scheduler driven by `clock`.
    pub fn new(clock: C) -> Self {
        let (task_tx, task_rx) = channel();
        Scheduler {
            clock,
            frame_id: 0,
            last_nanos: None,
            requester: FrameRequester::new(),
            task_tx,
            task_rx,
            idle_queue: Vec::new(),
            callbacks: std::array::from_fn(|_| Vec::new()),
            next_callback: 0,
        }
    }

    /// A cloneable, `Send` handle for marshaling work onto the UI thread.
    pub fn spawner(&self) -> Spawner {
        Spawner::new(self.task_tx.clone(), self.requester.clone())
    }

    /// A cloneable, `Send` handle for requesting frames (e.g. from animations).
    pub fn requester(&self) -> FrameRequester {
        self.requester.clone()
    }

    /// Requests that a frame be run.
    pub fn request_frame(&self) {
        self.requester.request_frame();
    }

    /// Whether a frame is pending (a request was made since the last frame).
    pub fn needs_frame(&self) -> bool {
        self.requester.is_requested()
    }

    /// The id the next frame will carry.
    pub fn frame_id(&self) -> FrameId {
        self.frame_id
    }

    /// Borrows the clock (e.g. to advance a `ManualClock` between frames).
    pub fn clock(&self) -> &C {
        &self.clock
    }

    /// Registers `callback` to run during `phase` on every frame. Returns an id
    /// for [`remove`](Scheduler::remove).
    pub fn on_phase(
        &mut self,
        phase: Phase,
        callback: impl FnMut(&FrameInfo) + 'static,
    ) -> CallbackId {
        let id = CallbackId(self.next_callback);
        self.next_callback += 1;
        self.callbacks[phase.index()].push(Entry {
            id,
            callback: Box::new(callback),
        });
        id
    }

    /// Removes a previously-registered callback. No-op if already gone.
    pub fn remove(&mut self, id: CallbackId) {
        for phase in &mut self.callbacks {
            phase.retain(|e| e.id != id);
        }
    }

    /// Runs exactly one frame: clears the request flag, drains marshaled tasks,
    /// then runs every phase in order, then idle tasks.
    ///
    /// Callbacks may call `request_frame`/`spawn` (via their captured handles) to
    /// schedule the *next* frame; those requests are observed after this returns.
    pub fn run_frame(&mut self) {
        self.requester.take(); // clear; callbacks may re-request for next frame

        let now = self.clock.now_nanos();
        let delta = self.last_nanos.map_or(0, |last| now.saturating_sub(last));
        self.last_nanos = Some(now);
        let info = FrameInfo {
            id: self.frame_id,
            now_nanos: now,
            delta_nanos: delta,
        };
        self.frame_id += 1;

        self.drain_tasks();

        for phase in Phase::ordered() {
            self.run_phase(phase, &info);
        }

        // Idle work runs after the frame's visible output is committed.
        let idle = std::mem::take(&mut self.idle_queue);
        for task in idle {
            task();
        }
    }

    /// Pulls all queued tasks: High/Normal run now (High first), Idle is stashed
    /// for after the frame.
    fn drain_tasks(&mut self) {
        let mut high = Vec::new();
        let mut normal = Vec::new();
        while let Ok((priority, task)) = self.task_rx.try_recv() {
            match priority {
                Priority::High => high.push(task),
                Priority::Normal => normal.push(task),
                Priority::Idle => self.idle_queue.push(task),
            }
        }
        for task in high.into_iter().chain(normal) {
            task();
        }
    }

    fn run_phase(&mut self, phase: Phase, info: &FrameInfo) {
        let idx = phase.index();
        // Take the list out so callbacks can't alias the table while running.
        let mut entries = std::mem::take(&mut self.callbacks[idx]);
        for entry in entries.iter_mut() {
            (entry.callback)(info);
        }
        // Restore, keeping any entries added during the run (none can be, today,
        // since callbacks hold no &mut Scheduler — but this stays correct if that
        // changes).
        let added = std::mem::take(&mut self.callbacks[idx]);
        entries.extend(added);
        self.callbacks[idx] = entries;
    }
}
