//! Exact-session terminal input recovery composition.

use std::path::Path;

use crate::{
    adapters::{
        process::SystemProcessReplacer,
        runtime::input_recovery::{
            ExecutableIdentity, InputRecovery, RecoveryError, RecoveryFailure,
        },
    },
    domain::SessionId,
};

use super::super::TerminalError;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum RecoveryDisposition {
    #[default]
    None,
    Replace,
    FailClosed(RecoveryFailure),
}

impl RecoveryDisposition {
    pub(super) const fn is_recovery_shutdown(self) -> bool {
        !matches!(self, Self::None)
    }

    pub(super) const fn failure_reason(self) -> RecoveryFailure {
        match self {
            Self::FailClosed(reason) => reason,
            Self::None | Self::Replace => RecoveryFailure::PreparationFailed,
        }
    }
}

pub(super) fn unavailable(
    error: RecoveryError,
    executable: &Path,
    state_root: Option<&Path>,
    session_id: SessionId,
) -> TerminalError {
    failure(
        "automatic input recovery could not be admitted",
        error.failure(),
        executable,
        state_root,
        session_id,
    )
}

pub(super) fn stalled(
    reason: RecoveryFailure,
    executable: &Path,
    state_root: Option<&Path>,
    session_id: SessionId,
) -> TerminalError {
    failure(
        "terminal input became unresponsive",
        reason,
        executable,
        state_root,
        session_id,
    )
}

pub(super) fn preparation_failed(
    cause: &TerminalError,
    reason: RecoveryFailure,
    executable: &Path,
    state_root: Option<&Path>,
    session_id: SessionId,
    recovery_export: Option<&Path>,
) -> TerminalError {
    let mut message = format!(
        "terminal input became unresponsive ({reason}); recovery preparation failed: {cause}; exact resume command: {resume}",
        reason = reason.as_str(),
        resume = resume_command(executable, state_root, session_id),
    );
    if let Some(path) = recovery_export {
        message.push_str("; optimistic recovery file: ");
        message.push_str(&shell_quote(path));
    }
    TerminalError::Io(message)
}

pub(super) fn replace(
    recovery: &mut InputRecovery,
    executable: &Path,
    identity: &ExecutableIdentity,
    working_directory: &Path,
    state_root: Option<&Path>,
    session_id: SessionId,
) -> Result<SessionId, TerminalError> {
    let proof = recovery
        .prepare()
        .map_err(|error| unavailable(error, executable, state_root, session_id))?;
    record("recovery_prepared", None, recovery.attempt_count(), None);
    SystemProcessReplacer::replace_after_input_stall(
        executable,
        identity,
        working_directory,
        state_root,
        &proof,
    )
    .map_err(|(reason, message)| {
        record(
            "replacement",
            Some(reason),
            recovery.attempt_count(),
            Some("failed"),
        );
        TerminalError::Io(format!(
            "{message}; exact resume command: {}",
            resume_command(executable, state_root, proof.session_id)
        ))
    })?;
    Ok(session_id)
}

pub(super) fn record(
    stage: &'static str,
    reason: Option<RecoveryFailure>,
    attempt_count: usize,
    outcome: Option<&'static str>,
) {
    crate::adapters::diagnostics::record(crate::adapters::diagnostics::SafeEvent::InputRecovery {
        stage,
        reason: reason.map(RecoveryFailure::as_str),
        attempt_count,
        outcome,
    });
}

fn failure(
    prefix: &str,
    reason: RecoveryFailure,
    executable: &Path,
    state_root: Option<&Path>,
    session_id: SessionId,
) -> TerminalError {
    TerminalError::Io(format!(
        "{prefix} ({reason}); exact resume command: {}",
        resume_command(executable, state_root, session_id),
        reason = reason.as_str(),
    ))
}

fn resume_command(executable: &Path, state_root: Option<&Path>, session_id: SessionId) -> String {
    let mut command = shell_quote(executable);
    if let Some(root) = state_root {
        command.push_str(" --state-dir ");
        command.push_str(&shell_quote(root));
    }
    command.push_str(" -r ");
    command.push_str(&session_id.to_string());
    command
}

fn shell_quote(path: &Path) -> String {
    let value = path.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use crate::{
        adapters::{runtime::SystemIdGenerator, terminal::TerminalError},
        ports::environment::IdGenerator as _,
    };

    #[test]
    fn exact_resume_command_preserves_executable_state_root_and_session() {
        let mut ids = SystemIdGenerator;
        let session = ids.session_id();
        assert_eq!(
            super::resume_command(
                std::path::Path::new("/Applications/Proqi's bin/proqi"),
                Some(std::path::Path::new("/tmp/state root")),
                session,
            ),
            format!(
                "'/Applications/Proqi'\\''s bin/proqi' --state-dir '/tmp/state root' -r {session}"
            )
        );
    }

    #[test]
    fn failed_preparation_keeps_the_cause_and_exact_resume_command() {
        let mut ids = SystemIdGenerator;
        let session = ids.session_id();
        let error = super::preparation_failed(
            &TerminalError::Worker("runtime shutdown exceeded its shared deadline"),
            crate::adapters::runtime::input_recovery::RecoveryFailure::PreparationFailed,
            std::path::Path::new("/opt/proqi"),
            Some(std::path::Path::new("/tmp/state")),
            session,
            None,
        );

        let rendered = error.to_string();
        assert!(rendered.contains("runtime shutdown exceeded its shared deadline"));
        assert!(rendered.contains(&format!(
            "exact resume command: '/opt/proqi' --state-dir '/tmp/state' -r {session}"
        )));
    }
}
