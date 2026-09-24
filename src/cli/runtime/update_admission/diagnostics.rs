//! Private, content-free CLI diagnostics for external convergence.

use crate::cli::error_code::ErrorCode;
use serde_json::json;

use crate::{
    application::{ExternalUpgradeBlocker, ExternalUpgradeFailure},
    domain::{ExternalRestartPending, SessionId, StableVersion},
    ports::update::{ExternalUpgradeAuthorityError, UpdateError},
};

use super::CliError;

pub(super) fn obsolete_error(current: &StableVersion, observed: &StableVersion) -> CliError {
    CliError::new(
        ErrorCode::ObsoleteExecutable,
        format!(
            "this Proqi executable is version {current}, but the verified installation has already recorded version {observed}; start the active Proqi executable"
        ),
    )
    .with_details(json!({
        "current_version": current,
        "observed_installed_version": observed,
    }))
}

pub(super) fn convergence_active(exact_resume: Option<SessionId>) -> CliError {
    let recovery = exact_resume.map_or_else(
        || "retry with the active Proqi executable after the current convergence finishes".to_owned(),
        |session_id| {
            format!(
                "retry `proqi -r {session_id}` with the active Proqi executable and the same state root after the current convergence finishes"
            )
        },
    );
    CliError::new(
        ErrorCode::UpdateConvergenceActive,
        format!("Proqi cannot open the schema while update convergence is active; {recovery}"),
    )
}

pub(super) fn pending_target_error(
    current: &StableVersion,
    observed: &StableVersion,
    pending: &ExternalRestartPending,
) -> CliError {
    let sessions = pending
        .expectations()
        .iter()
        .map(|expectation| {
            json!({
                "session_id": expectation.session_id(),
                "previous_version": expectation.previous_version(),
            })
        })
        .collect::<Vec<_>>();
    let identities = pending
        .expectations()
        .iter()
        .map(|expectation| expectation.session_id().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    CliError::new(
        ErrorCode::ExternalUpgradePending,
        format!(
            "Proqi {current} cannot replace the unfinished external restart for installed version {observed}; restore the verified {observed} installation, resume these exact SessionIds, then retry the package upgrade: {identities}"
        ),
    )
    .with_details(json!({
        "current_version": current,
        "pending_target_version": pending.target_version(),
        "sessions": sessions,
    }))
}

pub(super) fn external_error(error: ExternalUpgradeFailure) -> CliError {
    match error {
        ExternalUpgradeFailure::Blocked(blockers) => blockers_error(
            ErrorCode::ExternalUpgradeBlocked,
            "verified live Proqi sessions block the external upgrade",
            blockers,
        ),
        ExternalUpgradeFailure::Capacity {
            blockers,
            participant_count,
            maximum,
        } => capacity_error(blockers, participant_count, maximum),
        ExternalUpgradeFailure::Incomplete(blockers) => incomplete_error(blockers),
        ExternalUpgradeFailure::Persistence { blockers, .. } => blockers_error(
            ErrorCode::ExternalUpgradePersistenceFailed,
            "the update cache could not record safe external convergence, so affected exact Proqi sessions were not admitted",
            blockers,
        ),
        ExternalUpgradeFailure::Authority { blockers, .. } => blockers_error(
            ErrorCode::ExternalUpgradeIncomplete,
            "external convergence lost verified schema or installation authority, so affected exact Proqi sessions require recovery",
            blockers,
        ),
        ExternalUpgradeFailure::AuthorityUnavailable(error) => authority_error(error),
        ExternalUpgradeFailure::Update(error) => external_update_error(error),
    }
}

fn incomplete_error(mut blockers: Vec<ExternalUpgradeBlocker>) -> CliError {
    blockers.sort_by_key(|blocker| blocker.session_id);
    let identities = blockers
        .iter()
        .map(|blocker| blocker.session_id.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    CliError::new(
        ErrorCode::ExternalUpgradeIncomplete,
        format!(
            "external convergence could not restore every exact Proqi session; after any listed live owner exits, resume each missing session with the active executable and the same state root using `proqi -r <SessionId>`: {identities}"
        ),
    )
    .with_details(json!({ "blockers": blocker_details(&blockers) }))
}

fn capacity_error(
    mut blockers: Vec<ExternalUpgradeBlocker>,
    participant_count: usize,
    maximum: usize,
) -> CliError {
    blockers.sort_by_key(|blocker| blocker.session_id);
    let details = blocker_details(&blockers);
    let close_count = participant_count.saturating_sub(maximum);
    CliError::new(
        ErrorCode::ExternalUpgradeCapacity,
        format!(
            "{participant_count} compatible live Proqi sessions exceed the external replacement limit of {maximum}; close at least {close_count} live session(s), then retry with the active Proqi executable"
        ),
    )
    .with_details(json!({
        "blockers": details,
        "participant_count": participant_count,
        "maximum": maximum,
        "blockers_truncated": participant_count > blockers.len(),
    }))
}

fn authority_error(error: ExternalUpgradeAuthorityError) -> CliError {
    match error {
        ExternalUpgradeAuthorityError::Runtime(error) => error.into(),
        ExternalUpgradeAuthorityError::Update(error) => external_update_error(error),
    }
}

fn external_update_error(error: UpdateError) -> CliError {
    match error {
        UpdateError::State(_) => update_state_error(error),
        UpdateError::Installation(_) => {
            CliError::new(ErrorCode::InstallationFailed, error.to_string())
        }
        _ => CliError::new(ErrorCode::UpdateCoordinationFailed, error.to_string()),
    }
}

fn blockers_error(
    code: ErrorCode,
    summary: &str,
    mut blockers: Vec<ExternalUpgradeBlocker>,
) -> CliError {
    blockers.sort_by_key(|blocker| blocker.session_id);
    let identities = blockers
        .iter()
        .map(|blocker| {
            let version = blocker
                .version
                .as_ref()
                .map_or_else(|| "invalid version".to_owned(), ToString::to_string);
            format!("{} ({version})", blocker.session_id)
        })
        .collect::<Vec<_>>()
        .join(", ");
    CliError::new(
        code,
        format!(
            "{summary}: {identities}; close the listed sessions, or update compatible sessions, then retry with the active Proqi executable"
        ),
    )
    .with_details(json!({ "blockers": blocker_details(&blockers) }))
}

fn blocker_details(blockers: &[ExternalUpgradeBlocker]) -> Vec<serde_json::Value> {
    blockers
        .iter()
        .map(|blocker| {
            json!({
                "instance_id": blocker.instance_id,
                "session_id": blocker.session_id,
                "version": blocker.version,
                "reason": blocker.reason.as_str(),
            })
        })
        .collect()
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Result::map_err transfers the owned update error into this adapter"
)]
pub(super) fn update_state_error(error: UpdateError) -> CliError {
    CliError::new(ErrorCode::UpdateStateFailed, error.to_string())
}

#[cfg(test)]
mod tests {
    use crate::{
        adapters::memory::FakeIdGenerator,
        application::{ExternalUpgradeBlocker, ExternalUpgradeBlockerReason},
        domain::StableVersion,
        ports::environment::IdGenerator as _,
    };

    use super::capacity_error;

    #[test]
    fn capacity_diagnostic_is_typed_actionable_and_reports_truncation() {
        let mut ids = FakeIdGenerator::new(1_801_100_000_000);
        let blocker = ExternalUpgradeBlocker {
            instance_id: ids.instance_id(),
            session_id: ids.session_id(),
            version: Some(StableVersion::parse("0.10.0").expect("version")),
            reason: ExternalUpgradeBlockerReason::CohortCapacityExceeded,
        };

        let diagnostic = format!("{:?}", capacity_error(vec![blocker], 33, 32));

        assert!(diagnostic.contains("external_upgrade_capacity"));
        assert!(diagnostic.contains("cohort_capacity_exceeded"));
        assert!(diagnostic.contains("close at least 1 live session"));
        assert!(diagnostic.contains("blockers_truncated"));
        assert!(diagnostic.contains("Bool(true)"));
    }
}
