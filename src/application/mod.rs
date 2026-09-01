//! Application state, normalized actions, effects, errors, and reducer.

mod action;
mod admission;
mod attachments;
mod capture;
mod control;
mod error;
mod instructional_text;
mod locks;
mod model;
mod mutations;
mod onboarding;
mod prompt;
mod recovery;
mod reducer;
mod rehydrate;
mod release_highlights;
mod service;
#[cfg(test)]
mod test_support;
mod update;
mod update_coordination;

pub use action::Action;
pub(crate) use action::{OwnedThoughtCreation, OwnedThoughtEdit};
pub use admission::{PendingMutationIntent, PendingMutationIntents};
pub use attachments::{
    AttachmentAccessibilityState, AttachmentPreflightOutcome, AttachmentRefreshCause,
    AttachmentRefreshOutcome, attachment_keys,
};
pub use capture::{apply_capture, prepare_capture};
pub(crate) use control::{ControlReplay, match_control_replay};
pub use error::{ApplicationError, ApplicationResult, FailureCode};
pub use model::{
    AppState, ClipboardIntent, DurabilityState, Effect, EmptyBoardTransition, InteractionMode,
    ScreenshotIntent, ScreenshotPauseReason, UpdateIntent,
};
pub use onboarding::{FirstRunEnvironment, first_run_board};
pub(crate) use prompt::{
    SHARED_PROMPT_STARTERS, SharedPromptStarter, join_prompt_for_target, supports_shared_starters,
};
pub use recovery::capture_recovery;
pub use reducer::reduce;
pub use release_highlights::{ReleaseHighlightPresentation, ReleaseHighlightSelection};
pub use service::{LeasedSession, SessionService, SessionServiceError, ThoughtMutation};
pub use update::{
    UpdateAvailability, UpdateCheckMode, UpdateCheckResult, UpdateRefresh, UpdateService,
};
pub(crate) use update_coordination::is_compatible_update_participant;
pub use update_coordination::{UpdateExecution, UpdateExecutionStatus, UpdateRestartCoordinator};
