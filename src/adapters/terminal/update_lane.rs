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
    domain::{Installation, InstallationKind, StableVersion, Timestamp},
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
}

pub(super) enum UpdateResult {
    Notice(UpdateNotice),
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
        cancellation: CancellationFlag,
    ) -> Self {
        let (request_sender, request_receiver) = sync_channel(4);
        let (result_sender, result_receiver) = sync_channel(4);
        let lifecycle = WorkerLifecycle::default();
        let worker_lifecycle = lifecycle.clone();
        let handle = thread::spawn(move || {
            worker_lifecycle.run("update", || {
                update_loop(
                    &request_receiver,
                    &result_sender,
                    &cache_directory,
                    installation.as_ref(),
                    &coordinator,
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
        self.lifecycle.failure("update")
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
            continue;
        };
        let result = match request {
            UpdateRequest::Check { enabled } => check(
                &state,
                installation,
                coordinator,
                enabled,
                &mut prompt_lease,
            )
            .ok()
            .map(UpdateResult::Notice),
            UpdateRequest::Act(intent) => {
                let action = act(&state, installation, coordinator, intent, &mut process);
                prompt_lease = None;
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

fn check(
    state: &FileUpdateStateStore,
    installation: &Installation,
    coordinator: &FileRuntimeCoordinator,
    enabled: bool,
    prompt_lease: &mut Option<Box<dyn UpdateLease>>,
) -> Result<UpdateNotice, UpdateError> {
    let installed = StableVersion::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|_| UpdateError::InvalidResponse)?;
    let detector = SystemInstallDetector::for_executable(installation.executable.clone());
    let mut source = GitHubReleaseSource::new();
    let result =
        crate::application::UpdateService::new(state, &mut source, &detector, &SystemClock).check(
            installed,
            UpdateCheckMode::Implicit {
                enabled,
                release_build: !cfg!(debug_assertions),
                interactive: true,
            },
        )?;
    if result.availability != UpdateAvailability::Available {
        return Err(UpdateError::Coordination("no_actionable_update".to_owned()));
    }
    let version = result.latest_version.ok_or(UpdateError::InvalidResponse)?;
    let Some(lease) = state.try_lock(installation.identity, UpdateLockKind::Prompt)? else {
        return Err(UpdateError::Coordination(
            "prompt_owned_elsewhere".to_owned(),
        ));
    };
    let participants = compatible_participants(coordinator, installation)?;
    *prompt_lease = Some(lease);
    Ok(UpdateNotice {
        version,
        installation: installation.kind,
        participants: participants.max(1),
    })
}

fn act(
    state: &FileUpdateStateStore,
    installation: &Installation,
    coordinator: &FileRuntimeCoordinator,
    intent: UpdateIntent,
    process: &mut SystemProcessRunner,
) -> Result<UpdateActionResult, UpdateError> {
    match intent {
        UpdateIntent::Dismiss(version) => {
            state.dismiss(installation.identity, version)?;
            Ok(UpdateActionResult::Dismissed)
        }
        UpdateIntent::Skip(version) => {
            state.skip(installation.identity, version)?;
            Ok(UpdateActionResult::Skipped)
        }
        UpdateIntent::ViewInstructions(version) => Ok(UpdateActionResult::Instructions(version)),
        UpdateIntent::Install(version) => {
            install(state, installation, coordinator, &version, process)
        }
    }
}

fn install(
    state: &FileUpdateStateStore,
    installation: &Installation,
    coordinator: &FileRuntimeCoordinator,
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
    let mut installer = HomebrewFormulaInstaller::new(process, active);
    let mut gateway = LocalUpdateControlClient::new(SystemIdGenerator);
    let now = SystemClock.now();
    let deadline = Timestamp::from_millis(now.as_millis().saturating_add(UPDATE_DEADLINE_MILLIS));
    let mut ids = SystemIdGenerator;
    let execution = UpdateRestartCoordinator::new(state, coordinator, &mut gateway, &mut installer)
        .execute(ids.request_id(), installation.identity, version, deadline)?;
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
    participant.control_protocol == Some(crate::ports::control::CONTROL_PROTOCOL_VERSION)
        && participant.control_endpoint.is_some()
        && participant.update.as_ref().is_some_and(|context| {
            context.installation_identity == installation.identity
                && context.protocol == crate::ports::update::UPDATE_CONTROL_PROTOCOL_VERSION
        })
}
