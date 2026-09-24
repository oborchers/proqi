//! Application state, normalized actions, effects, errors, and reducer.

mod action;
mod admission;
mod attachments;
mod capture;
mod control;
mod error;
mod history_contract;
mod instructional_text;
mod locks;
mod model;
mod mutations;
mod onboarding;
mod preconditions;
mod prompt;
mod recovery;
mod reducer;
mod rehydrate;
mod release_highlights;
mod service;
#[cfg(test)]
mod test_support;
pub(crate) mod text_reflow;
mod update;
mod update_coordination;

pub use action::Action;
pub(crate) use action::{OwnedThoughtCreation, OwnedThoughtEdit, OwnedThoughtReflow};
pub use admission::{PendingMutationIntent, PendingMutationIntents};
pub use attachments::{
    AttachmentAccessibilityState, AttachmentPreflightOutcome, AttachmentPresentationState,
    AttachmentRefreshCause, AttachmentRefreshOutcome, attachment_keys,
};
pub use capture::{apply_capture, prepare_capture};
pub(crate) use control::{ControlReplay, attach_control_fingerprint, match_control_replay};
pub use error::{ApplicationError, ApplicationResult, FailureCode};
pub(crate) use history_contract::UndoContract;
pub use model::{
    AppState, ClipboardIntent, DurabilityState, Effect, EmptyBoardTransition, HistoryResolution,
    InteractionMode, ScreenshotIntent, ScreenshotPauseReason, UpdateIntent,
};
pub(crate) use model::{SequencedMutationEffectError, SequencedMutationEffects};
pub use onboarding::{FirstRunEnvironment, first_run_board};
pub(crate) use preconditions::exact_live_thought;
pub(crate) use prompt::{
    SHARED_HARNESS_COMMANDS, join_prompt_for_target, supports_shared_commands,
};
pub use recovery::capture_recovery;
pub use reducer::reduce;
pub use release_highlights::{ReleaseHighlightPresentation, ReleaseHighlightSelection};
pub(crate) use service::derived_duplicate_item_ids;
pub use service::{
    BoardItemMutation, BrowserHistoryMovement, LeasedSession, NamedSession,
    NamedSessionDisposition, SessionAdministrationReceipt, SessionService, SessionServiceError,
    ThoughtMutation,
};
pub use update::{
    UpdateAvailability, UpdateCheckMode, UpdateCheckResult, UpdateRefresh, UpdateService,
};
pub(crate) use update_coordination::is_compatible_update_participant;
pub use update_coordination::{
    ExternalUpgradeAdmission, ExternalUpgradeBlocker, ExternalUpgradeBlockerReason,
    ExternalUpgradeCacheStatus, ExternalUpgradeCoordinator, ExternalUpgradeFailure,
    UpdateExecution, UpdateExecutionStatus, UpdateRestartCoordinator,
    admit_pending_external_resume,
};
