//! Frame identity, timing, and the ordered phase model.
//!
//! The phase order is a **frozen contract** (see audit R2): everything that
//! plugs into a frame — animation ticks, layout, the native commit — relies on
//! running at a well-defined point relative to the others.

/// Monotonically increasing frame counter.
pub type FrameId = u64;

/// Timing handed to every frame callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameInfo {
    /// This frame's id.
    pub id: FrameId,
    /// Clock time at the start of this frame, in nanoseconds.
    pub now_nanos: u64,
    /// Nanoseconds since the previous frame (`0` for the first frame).
    pub delta_nanos: u64,
}

impl FrameInfo {
    /// Time since the previous frame, in fractional seconds — handy for
    /// animation integration.
    pub fn delta_secs(&self) -> f32 {
        self.delta_nanos as f32 / 1_000_000_000.0
    }
}

/// The ordered phases of a single frame. Callbacks run in this exact order.
///
/// - `PreFrame`: drain marshaled tasks / input already happened; prepare state.
/// - `Frame`: advance animations (which write signals → synchronous effects).
/// - `Layout`: compute geometry from the now-settled tree.
/// - `Commit`: flush the accumulated command buffer to the native backend.
/// - `PostFrame`: bookkeeping, idle work.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Before anything else this frame.
    PreFrame,
    /// Animation / time-driven updates.
    Frame,
    /// Layout computation.
    Layout,
    /// Native commit (single batched buffer flush).
    Commit,
    /// After commit: bookkeeping, idle callbacks.
    PostFrame,
}

/// Number of distinct phases; used to size the per-phase callback table.
pub(crate) const PHASE_COUNT: usize = 5;

impl Phase {
    /// All phases in execution order.
    pub(crate) fn ordered() -> [Phase; PHASE_COUNT] {
        [
            Phase::PreFrame,
            Phase::Frame,
            Phase::Layout,
            Phase::Commit,
            Phase::PostFrame,
        ]
    }

    pub(crate) fn index(self) -> usize {
        match self {
            Phase::PreFrame => 0,
            Phase::Frame => 1,
            Phase::Layout => 2,
            Phase::Commit => 3,
            Phase::PostFrame => 4,
        }
    }
}
