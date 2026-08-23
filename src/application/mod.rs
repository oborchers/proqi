//! Application state, normalized actions, effects, errors, and reducer.

mod control;
mod model;
mod mutations;
mod recovery;
mod reducer;
mod rehydrate;
mod service;

pub(crate) use control::{ControlReplay, match_control_replay};
pub use model::{
    Action, AppState, ApplicationError, ApplicationResult, ClipboardIntent, DurabilityState,
    Effect, FailureCode, InteractionMode,
};
pub use recovery::capture_recovery;
pub use reducer::reduce;
pub use service::{LeasedSession, SessionService, SessionServiceError, ThoughtMutation};
