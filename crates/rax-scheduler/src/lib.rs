//! The `rax` frame scheduler: the spine connecting reactivity, layout, and the
//! native commit.
//!
//! # Why this exists (audit R2)
//!
//! Reactive effects run *synchronously* on write, so derived state is always
//! consistent. But pushing those changes to native views must be **coalesced to
//! one commit per frame**, aligned to the display refresh, with layout running
//! in between. The scheduler owns that frame pipeline and its [`Phase`] order:
//!
//! ```text
//!  PreFrame → Frame → Layout → Commit → PostFrame
//!  (tasks)   (anim)   (geom)   (flush)  (idle)
//! ```
//!
//! It is also the **thread-marshaling boundary**: a [`Spawner`] is `Send`, so
//! work from any thread is enqueued and then executed on the UI thread at a
//! frame boundary — the safe way for an async result to write signals.
//!
//! # Driving it
//!
//! The scheduler does not run itself. A platform driver calls
//! [`run_frame`](Scheduler::run_frame) when [`needs_frame`](Scheduler::needs_frame)
//! is set (on `Choreographer`/`CADisplayLink`). Tests use [`ManualClock`] and
//! call `run_frame` directly.

#![forbid(unsafe_code)]

mod clock;
mod frame;
mod scheduler;
mod task;

pub use clock::{Clock, ManualClock, SystemClock};
pub use frame::{FrameId, FrameInfo, Phase};
pub use scheduler::{CallbackId, Scheduler};
pub use task::{FrameRequester, Priority, Spawner};
