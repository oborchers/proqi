//! Application state, normalized actions, effects, errors, and reducer.

mod control;
mod error;
mod model;
mod mutations;
mod recovery;
mod reducer;
mod rehydrate;
mod service;
#[cfg(test)]
mod test_support;
mod update;
mod update_coordination;

pub(crate) use control::{ControlReplay, match_control_replay};
pub use error::{ApplicationError, ApplicationResult, FailureCode};
pub use model::{
    Action, AppState, ClipboardIntent, DurabilityState, Effect, InteractionMode, UpdateIntent,
};
pub use recovery::capture_recovery;
pub use reducer::reduce;
pub use service::{LeasedSession, SessionService, SessionServiceError, ThoughtMutation};
pub use update::{
    UpdateAvailability, UpdateCheckMode, UpdateCheckResult, UpdateRefresh, UpdateService,
};
pub use update_coordination::{UpdateExecution, UpdateExecutionStatus, UpdateRestartCoordinator};
