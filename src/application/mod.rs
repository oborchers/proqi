//! Application state, normalized actions, effects, errors, and reducer.

mod action;
mod admission;
mod attachments;
mod capture;
mod control;
mod error;
mod locks;
mod model;
mod mutations;
mod prompt;
mod recovery;
mod reducer;
mod rehydrate;
mod service;
#[cfg(test)]
mod test_support;
mod update;
mod update_coordination;

pub use action::Action;
pub use admission::{PendingMutationIntent, PendingMutationIntents};
pub use attachments::{
    AttachmentAccessibilityState, AttachmentPreflightOutcome, AttachmentRefreshCause,
    AttachmentRefreshOutcome, attachment_keys,
};
pub use capture::{apply_capture, prepare_capture};
pub(crate) use control::{ControlReplay, match_control_replay};
pub use error::{ApplicationError, ApplicationResult, FailureCode};
pub use model::{
    AppState, ClipboardIntent, DurabilityState, Effect, InteractionMode, ScreenshotIntent,
    ScreenshotPauseReason, UpdateIntent,
};
pub(crate) use prompt::{
    SHARED_PROMPT_STARTERS, SharedPromptStarter, join_prompt_for_target, supports_shared_starters,
};
pub use recovery::capture_recovery;
pub use reducer::reduce;
pub use service::{LeasedSession, SessionService, SessionServiceError, ThoughtMutation};
pub use update::{
    UpdateAvailability, UpdateCheckMode, UpdateCheckResult, UpdateRefresh, UpdateService,
};
pub(crate) use update_coordination::is_compatible_update_participant;
pub use update_coordination::{UpdateExecution, UpdateExecutionStatus, UpdateRestartCoordinator};
