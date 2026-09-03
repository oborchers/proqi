//! Bounded background update checks, prompt election, and installation coordination.

use std::{
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel},
    thread::{self, JoinHandle},
};

use crate::{
    adapters::{
        control::LocalUpdateControlClient,
        process::{CancellationFlag, SystemProcessRunner},
        runtime::{FileRuntimeCoordinator, SystemClock, SystemIdGenerator},
        update::{
            FileUpdateStateStore, GitHubReleaseSource, HomebrewFormulaInstaller,
            SystemInstallDetector,
        },
    },
    application::{
        Effect, UpdateAvailability, UpdateCheckMode, UpdateIntent, UpdateRestartCoordinator,
    },
    domain::{Installation, InstallationKind, InstanceId, StableVersion, Timestamp},
    ports::{
        environment::{Clock as _, IdGenerator as _},
        runtime::InstanceInfo,
        update::{UpdateError, UpdateLease, UpdateLockKind, UpdateStateStore as _},
    },
};

use super::{
    TerminalError,
    supervisor::{ShutdownDeadline, WorkerLifecycle, join_before},
};

#[cfg(test)]
mod tests;

const UPDATE_DEADLINE_MILLIS: i64 = 45_000;

enum UpdateRequest {
    Check { enabled: bool },
    Act(UpdateIntent),
}

pub(super) struct UpdateNotice {
    pub(super) version: StableVersion,
    pub(super) installation: InstallationKind,
    pub(super) participants: usize,
}

pub(super) enum UpdateActionResult {
    Dismissed,
    Skipped,
    Instructions(StableVersion),
    Executed(crate::application::UpdateExecution),
    HighlightsAcknowledged(Result<(), UpdateError>),
}

pub(super) enum ManualCheckResult {
    Current(StableVersion),
    Suppressed(StableVersion),
    InProgress,
    Instructions(StableVersion),
    Prompt(UpdateNotice),
}

pub(super) enum UpdateResult {
    Notice(UpdateNotice),
    ManualCheck(Result<ManualCheckResult, UpdateError>),
    Action(Result<UpdateActionResult, UpdateError>),
}

pub(super) struct UpdateLane {
    sender: Option<SyncSender<UpdateRequest>>,
    pub(super) receiver: Receiver<UpdateResult>,
    handle: Option<JoinHandle<()>>,
    lifecycle: WorkerLifecycle,
}

impl UpdateLane {
    pub(super) fn spawn(
        cache_directory: PathBuf,
        installation: Option<Installation>,
        coordinator: FileRuntimeCoordinator,
        initiating_instance: InstanceId,
        cancellation: CancellationFlag,
    ) -> Self {
        let (request_sender, request_receiver) = sync_channel(4);
        let (result_sender, result_receiver) = sync_channel(4);
        let lifecycle = WorkerLifecycle::default();
        let worker_lifecycle = lifecycle.clone();
        let handle = thread::spawn(move || {
            worker_lifecycle.run(super::supervisor::WorkerRole::Update, || {
                update_loop(
                    &request_receiver,
                    &result_sender,
                    &cache_directory,
                    installation.as_ref(),
                    &coordinator,
                    initiating_instance,
                    cancellation,
                );
            });
        });
        Self {
            sender: Some(request_sender),
            receiver: result_receiver,
            handle: Some(handle),
            lifecycle,
        }
    }

    pub(super) fn check(&self, enabled: bool) -> Result<(), TerminalError> {
        self.send_request(UpdateRequest::Check { enabled })
    }

    pub(super) fn send(&self, effect: &Effect) -> Result<bool, TerminalError> {
        let Effect::Update(intent) = effect else {
            return Ok(false);
        };
        self.send_request(UpdateRequest::Act(intent.clone()))?;
        Ok(true)
    }

    fn send_request(&self, request: UpdateRequest) -> Result<(), TerminalError> {
        self.sender
            .as_ref()
            .ok_or(TerminalError::Worker("update lane is closed"))?
            .try_send(request)
            .map_err(|error| map_send_error(&error))
    }

    pub(super) fn request_stop(&mut self) {
        self.lifecycle.request_stop();
        self.sender = None;
    }

    pub(super) fn worker_failure(&self) -> Option<TerminalError> {
        self.lifecycle
            .failure(super::supervisor::WorkerRole::Update)
    }

    pub(super) fn stopped_cleanly(&self) -> bool {
        self.lifecycle.stopped_cleanly()
    }

    pub(super) fn stop(mut self, deadline: ShutdownDeadline) -> Result<(), TerminalError> {
        self.request_stop();
        while !deadline.expired() {
            match self.receiver.recv_timeout(deadline.remaining()) {
                Ok(_) => {}
                Err(RecvTimeoutError::Disconnected | RecvTimeoutError::Timeout) => break,
            }
        }
        join_before(
            self.handle.take(),
            deadline,
            "update lane panicked",
            "update lane did not stop before the shutdown deadline",
        )
    }
}

impl Drop for UpdateLane {
    fn drop(&mut self) {
        self.request_stop();
    }
}

fn map_send_error(error: &TrySendError<UpdateRequest>) -> TerminalError {
    match error {
        TrySendError::Full(_) => TerminalError::Worker("update lane is full"),
        TrySendError::Disconnected(_) => TerminalError::Worker("update lane disconnected"),
    }
}

fn update_loop(
    requests: &Receiver<UpdateRequest>,
    results: &SyncSender<UpdateResult>,
    cache_directory: &Path,
    installation: Option<&Installation>,
    coordinator: &FileRuntimeCoordinator,
    initiating_instance: InstanceId,
    cancellation: CancellationFlag,
) {
    let Ok(state) = FileUpdateStateStore::new(cache_directory) else {
        while requests.recv().is_ok() {}
        return;
    };
    let mut prompt_lease: Option<Box<dyn UpdateLease>> = None;
    let mut process = SystemProcessRunner::cancellable(cancellation);
    while let Ok(request) = requests.recv() {
        let Some(installation) = installation else {
            let result = match request {
                UpdateRequest::Check { .. } => None,
                UpdateRequest::Act(UpdateIntent::CheckNow) => Some(UpdateResult::ManualCheck(Err(
                    UpdateError::Installation("installation could not be identified".to_owned()),
                ))),
                UpdateRequest::Act(_) => Some(UpdateResult::Action(Err(
                    UpdateError::Installation("installation could not be identified".to_owned()),
                ))),
            };
            if let Some(result) = result
                && results.send(result).is_err()
            {
                return;
            }
            continue;
        };
        let result = match request {
            UpdateRequest::Check { enabled } => match check(
                &state,
                installation,
                coordinator,
                enabled,
                &mut prompt_lease,
            ) {
                Ok(notice) => notice.map(UpdateResult::Notice),
                Err(error) => {
                    record_check_failure("startup", &error);
                    None
                }
            },
            UpdateRequest::Act(UpdateIntent::CheckNow) => {
                let result = check_now(&state, installation, coordinator, &mut prompt_lease);
                if let Err(error) = &result {
                    record_check_failure("manual", error);
                }
                Some(UpdateResult::ManualCheck(result))
            }
            UpdateRequest::Act(intent) => {
                let concludes_prompt = concludes_prompt(&intent);
                let action = act(
                    &state,
                    installation,
                    coordinator,
                    initiating_instance,
                    intent,
                    &mut process,
                );
                if concludes_prompt {
                    prompt_lease = None;
                }
                Some(UpdateResult::Action(action))
            }
        };
        if let Some(result) = result
            && results.send(result).is_err()
        {
            return;
        }
    }
}

const fn concludes_prompt(intent: &UpdateIntent) -> bool {
    matches!(
        intent,
        UpdateIntent::Dismiss(_)
            | UpdateIntent::Skip(_)
            | UpdateIntent::ViewInstructions(_)
            | UpdateIntent::Install(_)
    )
}

fn check_now(
    state: &FileUpdateStateStore,
    installation: &Installation,
    coordinator: &FileRuntimeCoordinator,
    prompt_lease: &mut Option<Box<dyn UpdateLease>>,
) -> Result<ManualCheckResult, UpdateError> {
    let installed = StableVersion::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|_| UpdateError::InvalidResponse)?;
    let detector = SystemInstallDetector::for_executable(installation.executable.clone());
    let mut source = GitHubReleaseSource::new();
    let result =
        crate::application::UpdateService::new(state, &mut source, &detector, &SystemClock)
            .check(installed.clone(), UpdateCheckMode::Explicit)?;
    if result.refresh == crate::application::UpdateRefresh::InProgress {
        return Ok(ManualCheckResult::InProgress);
    }
    let Some(version) = result.latest_version else {
        return Ok(ManualCheckResult::Current(installed));
    };
    match result.availability {
        UpdateAvailability::Current => Ok(ManualCheckResult::Current(installed)),
        UpdateAvailability::Suppressed => Ok(ManualCheckResult::Suppressed(version)),
        UpdateAvailability::Available if installation.kind == InstallationKind::SourceOrUnknown => {
            Ok(ManualCheckResult::Instructions(version))
        }
        UpdateAvailability::Available => {
            let Some(lease) = state.try_lock(installation.identity, UpdateLockKind::Prompt)? else {
                return Ok(ManualCheckResult::InProgress);
            };
            let participants = compatible_participants(coordinator, installation)?;
            *prompt_lease = Some(lease);
            Ok(ManualCheckResult::Prompt(UpdateNotice {
                version,
                installation: installation.kind,
                participants: participants.max(1),
            }))
        }
    }
}

fn check(
    state: &FileUpdateStateStore,
    installation: &Installation,
    coordinator: &FileRuntimeCoordinator,
    enabled: bool,
    prompt_lease: &mut Option<Box<dyn UpdateLease>>,
) -> Result<Option<UpdateNotice>, UpdateError> {
    let installed = StableVersion::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|_| UpdateError::InvalidResponse)?;
    let detector = SystemInstallDetector::for_executable(installation.executable.clone());
    let mut source = GitHubReleaseSource::new();
    let result =
        crate::application::UpdateService::new(state, &mut source, &detector, &SystemClock).check(
            installed,
            UpdateCheckMode::Startup {
                enabled,
                release_build: !cfg!(debug_assertions),
                interactive: true,
            },
        )?;
    if result.availability != UpdateAvailability::Available {
        return Ok(None);
    }
    let version = result.latest_version.ok_or(UpdateError::InvalidResponse)?;
    let Some(lease) = state.try_lock(installation.identity, UpdateLockKind::Prompt)? else {
        return Ok(None);
    };
    let participants = compatible_participants(coordinator, installation)?;
    *prompt_lease = Some(lease);
    Ok(Some(UpdateNotice {
        version,
        installation: installation.kind,
        participants: participants.max(1),
    }))
}

fn record_check_failure(mode: &'static str, error: &UpdateError) {
    let code = match error {
        UpdateError::Network => "network",
        UpdateError::InvalidResponse => "invalid_response",
        UpdateError::ResponseTooLarge => "response_too_large",
        UpdateError::Installation(_) => "installation",
        UpdateError::State(_) => "state",
        UpdateError::Coordination(_) => "coordination",
        UpdateError::InstallerFailed => "installer",
    };
    crate::adapters::diagnostics::record(
        crate::adapters::diagnostics::SafeEvent::UpdateCheckFailed { mode, code },
    );
}

fn act(
    state: &FileUpdateStateStore,
    installation: &Installation,
    coordinator: &FileRuntimeCoordinator,
    initiating_instance: InstanceId,
    intent: UpdateIntent,
    process: &mut SystemProcessRunner,
) -> Result<UpdateActionResult, UpdateError> {
    match intent {
        UpdateIntent::CheckNow => Err(UpdateError::Coordination(
            "update_check_routed_as_action".to_owned(),
        )),
        UpdateIntent::Dismiss(version) => {
            state.dismiss(installation.identity, version)?;
            Ok(UpdateActionResult::Dismissed)
        }
        UpdateIntent::Skip(version) => {
            state.skip(installation.identity, version)?;
            Ok(UpdateActionResult::Skipped)
        }
        UpdateIntent::ViewInstructions(version) => Ok(UpdateActionResult::Instructions(version)),
        UpdateIntent::AcknowledgeReleaseHighlights(announcement) => {
            Ok(highlight_acknowledgement_result(
                state.acknowledge_release_highlights(installation.identity, &announcement),
            ))
        }
        UpdateIntent::Install(version) => install(
            state,
            installation,
            coordinator,
            initiating_instance,
            &version,
            process,
        ),
    }
}

fn highlight_acknowledgement_result(result: Result<bool, UpdateError>) -> UpdateActionResult {
    UpdateActionResult::HighlightsAcknowledged(result.map(|_changed| ()))
}

fn install(
    state: &FileUpdateStateStore,
    installation: &Installation,
    coordinator: &FileRuntimeCoordinator,
    initiating_instance: InstanceId,
    version: &StableVersion,
    process: &mut SystemProcessRunner,
) -> Result<UpdateActionResult, UpdateError> {
    if installation.kind != InstallationKind::HomebrewFormula {
        return Err(UpdateError::Installation(
            "automatic installation requires Homebrew".to_owned(),
        ));
    }
    let active = installation
        .restart_executable
        .clone()
        .ok_or_else(|| UpdateError::Installation("active Homebrew path is absent".to_owned()))?;
    let cancellation = process.cancellation();
    let mut installer = HomebrewFormulaInstaller::new(process, active);
    let mut gateway =
        LocalUpdateControlClient::cancellable(SystemIdGenerator, cancellation.clone());
    let now = SystemClock.now();
    let deadline = Timestamp::from_millis(now.as_millis().saturating_add(UPDATE_DEADLINE_MILLIS));
    let mut ids = SystemIdGenerator;
    let execution = UpdateRestartCoordinator::new(state, coordinator, &mut gateway, &mut installer)
        .execute(
            ids.request_id(),
            initiating_instance,
            installation.identity,
            version,
            deadline,
            &cancellation,
        );
    match &execution {
        Ok(execution) => crate::adapters::diagnostics::record_update_execution(execution),
        Err(error) => crate::adapters::diagnostics::record_update_error(error),
    }
    let execution = execution?;
    Ok(UpdateActionResult::Executed(execution))
}

fn compatible_participants(
    coordinator: &FileRuntimeCoordinator,
    installation: &Installation,
) -> Result<usize, UpdateError> {
    use crate::ports::update::UpdateInstanceRegistry as _;

    Ok(coordinator
        .active_instances()?
        .into_iter()
        .filter(|participant| compatible(participant, installation))
        .count())
}

fn compatible(participant: &InstanceInfo, installation: &Installation) -> bool {
    crate::application::is_compatible_update_participant(participant, installation.identity)
}
