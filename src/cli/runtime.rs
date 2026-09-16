//! Platform composition for paths, schema coordination, and SQLite.

use std::path::{Path, PathBuf};

use crate::{
    adapters::{
        diagnostics::{SafeEvent, SchemaLifecycleStage},
        runtime::{
            FileRuntimeCoordinator, FileSchemaLease, NativePaths, SystemClock, SystemEnvironment,
            SystemIdGenerator, input_recovery::ExecutableIdentity,
        },
        sqlite::{SqliteStore, StoreConfig},
        terminal::TerminalResources,
        update::SystemInstallDetector,
    },
    application::LeasedSession,
    domain::{Installation, InstanceId, StableVersion, Timestamp},
    ports::{
        environment::{AppPaths, Clock, Environment, IdGenerator, Paths},
        runtime::RuntimeCoordinator,
        store::{MigrationMode, StoreError},
        update::{InstallDetector as _, UPDATE_CONTROL_PROTOCOL_VERSION},
    },
};

use super::output::CliError;

mod installation_authority;
mod update_admission;

#[cfg(test)]
use installation_authority::RuntimeExternalUpgradeAuthority;
use installation_authority::{VerifiedStartupInstallation, admit_runtime_startup};
use update_admission::StartupAdmission;

pub(super) struct RuntimeContext {
    pub(super) store: SqliteStore,
    pub(super) coordinator: FileRuntimeCoordinator,
    pub(super) clock: SystemClock,
    pub(super) ids: SystemIdGenerator,
    pub(super) cwd: PathBuf,
    config_dir: PathBuf,
    recovery_dir: PathBuf,
    attachment_dir: PathBuf,
    cache_dir: PathBuf,
    state_root: Option<PathBuf>,
    executable: PathBuf,
    installation: Option<crate::domain::Installation>,
    schema_lease: FileSchemaLease,
    startup_admission: StartupAdmission,
}

impl RuntimeContext {
    pub(super) fn open(
        state_root: Option<&Path>,
        resume_reference: Option<&str>,
    ) -> Result<Self, CliError> {
        let exact_resume = resume_reference.and_then(|reference| reference.parse().ok());
        let cwd = SystemEnvironment
            .current_directory()
            .map_err(|error| CliError::new("environment_failed", error.to_string(), 1))?;
        let paths = resolve_paths(state_root)?;
        let executable = current_executable()?;
        prepare_state_paths(&paths, state_root)?;
        let clock = SystemClock;
        let mut ids = SystemIdGenerator;
        let instance_id = initialize_runtime_diagnostics(&paths.data_dir, &mut ids)?;
        let config_dir = paths.config_dir.clone();
        let recovery_dir = paths.data_dir.join("recovery");
        let attachment_dir = paths.data_dir.join("attachments");
        let cache_dir = paths.cache_dir.clone();
        let (installation, current, initial_executable_identity) =
            detect_startup_installation(&executable)?;
        let coordinator = FileRuntimeCoordinator::new(
            paths.runtime_dir,
            instance_id,
            cwd.clone(),
            clock.now(),
            env!("CARGO_PKG_VERSION"),
        )?;
        let (replacement, follows_schema_change) =
            crate::adapters::process::replacement_startup_context();
        let startup_replacement = replacement.clone();
        let coordinator = coordinator.with_update_context(
            installation.identity,
            UPDATE_CONTROL_PROTOCOL_VERSION,
            replacement,
        );
        let (mut startup_admission, executable_identity) = admit_runtime_startup(
            &cache_dir,
            &coordinator,
            &executable,
            &installation,
            &current,
            initial_executable_identity,
            startup_replacement.as_ref(),
            exact_resume,
            &mut ids,
            clock.now(),
        )?;
        let verified = VerifiedStartupInstallation::new(
            &executable,
            &installation,
            executable_identity.as_ref(),
        );
        verified.revalidate()?;
        let (store, schema_lease) = open_store(
            &coordinator,
            &paths.data_dir,
            clock.now(),
            follows_schema_change,
            &mut startup_admission,
            verified,
        )?;
        verified.revalidate()?;
        startup_admission.finish_after_store_ready()?;
        record_schema_stage(SchemaLifecycleStage::Ready);
        Ok(Self {
            store,
            coordinator,
            clock,
            ids,
            cwd,
            config_dir,
            recovery_dir,
            attachment_dir,
            cache_dir,
            state_root: state_root.map(Path::to_path_buf),
            executable,
            installation: Some(installation),
            schema_lease,
            startup_admission,
        })
    }

    pub(super) fn terminal_settings(
        &self,
    ) -> Result<crate::adapters::terminal::LoadedSettings, crate::adapters::terminal::TerminalError>
    {
        crate::adapters::terminal::load_settings(&self.config_dir)
    }

    pub(super) fn into_terminal(
        self,
        session: LeasedSession<crate::adapters::runtime::FileSessionLease>,
        settings: crate::adapters::terminal::LoadedSettings,
    ) -> TerminalResources {
        let (state, session_lease) = session.into_parts();
        let attachment_directory = self.attachment_dir.join(state.board.session.id.to_string());
        TerminalResources {
            state,
            store: self.store,
            coordinator: self.coordinator,
            clock: self.clock,
            ids: self.ids,
            cwd: self.cwd,
            session_lease,
            schema_lease: self.schema_lease,
            settings,
            recovery_directory: self.recovery_dir,
            attachment_directory,
            installation: self.installation,
            cache_directory: self.cache_dir,
            state_root: self.state_root,
            executable: self.executable,
            startup_admission: self.startup_admission.into_lease(),
        }
    }

    pub(super) fn finish_exact_resume(
        &mut self,
        session_id: crate::domain::SessionId,
    ) -> Result<(), CliError> {
        self.startup_admission.finish_exact_resume(session_id)
    }
}

fn current_executable() -> Result<PathBuf, CliError> {
    SystemEnvironment
        .current_executable()
        .map_err(|error| CliError::new("environment_failed", error.to_string(), 1))
}

fn initialize_runtime_diagnostics(
    data_dir: &Path,
    ids: &mut impl IdGenerator,
) -> Result<InstanceId, CliError> {
    let instance_id = ids.instance_id();
    crate::adapters::diagnostics::initialize(data_dir, instance_id)
        .map_err(|error| CliError::new("diagnostics_failed", error.to_string(), 1))?;
    crate::adapters::diagnostics::record(SafeEvent::RuntimeOpening { instance_id });
    Ok(instance_id)
}

fn detect_startup_installation(
    executable: &Path,
) -> Result<(Installation, StableVersion, Option<ExecutableIdentity>), CliError> {
    let installation = SystemInstallDetector::for_executable(executable.to_path_buf())
        .detect()
        .map_err(|error| CliError::new("installation_failed", error.to_string(), 1))?;
    let executable_identity = (installation.kind
        == crate::domain::InstallationKind::StandaloneArchive)
        .then(|| ExecutableIdentity::read(executable))
        .transpose()
        .map_err(|error| {
            CliError::new(
                "installation_failed",
                format!(
                    "verified standalone executable identity is unavailable: {}",
                    error.failure().as_str()
                ),
                1,
            )
        })?;
    let current = StableVersion::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| CliError::new("invalid_build_version", error.to_string(), 1))?;
    Ok((installation, current, executable_identity))
}

fn prepare_state_paths(paths: &AppPaths, state_root: Option<&Path>) -> Result<(), CliError> {
    let mut directories = Vec::with_capacity(5);
    if let Some(root) = state_root {
        directories.push(root);
    }
    directories.extend([
        paths.data_dir.as_path(),
        paths.config_dir.as_path(),
        paths.cache_dir.as_path(),
        paths.runtime_dir.as_path(),
    ]);
    crate::adapters::filesystem::prepare_private_dirs(&directories).map_err(|error| {
        CliError::new(
            "unsafe_state_path",
            format!("Proqi state paths are unsafe: {error}"),
            2,
        )
    })
}

pub(super) fn resolve_paths(state_root: Option<&Path>) -> Result<AppPaths, CliError> {
    if let Some(root) = state_root {
        if !root.is_absolute() {
            return Err(CliError::input(format!(
                "state directory must be absolute: {}",
                root.display()
            )));
        }
        return Ok(AppPaths {
            data_dir: root.join("data"),
            config_dir: root.join("config"),
            cache_dir: root.join("cache"),
            runtime_dir: root.join("runtime"),
        });
    }
    NativePaths
        .resolve()
        .map_err(|error| CliError::new("environment_failed", error.to_string(), 1))
}

fn open_store(
    coordinator: &FileRuntimeCoordinator,
    data_dir: &Path,
    now: Timestamp,
    follows_schema_change: bool,
    startup_admission: &mut StartupAdmission,
    verified: VerifiedStartupInstallation<'_>,
) -> Result<(SqliteStore, FileSchemaLease), CliError> {
    if startup_admission.requires_exclusive_store() {
        return open_store_for_external_adoption(
            coordinator,
            data_dir,
            now,
            follows_schema_change,
            startup_admission,
            verified,
        );
    }
    let database = data_dir.join("proqi.sqlite3");
    let backups = data_dir.join("backups");
    let shared = coordinator.acquire_schema_shared()?;
    let refuse = StoreConfig::new(
        database.clone(),
        backups.clone(),
        MigrationMode::Refuse,
        now,
    );
    match SqliteStore::open(&refuse) {
        Ok(store) => {
            if follows_schema_change {
                record_schema_stage(SchemaLifecycleStage::FollowerRevalidated);
            }
            Ok((store, shared))
        }
        Err(StoreError::MigrationRequired { .. }) => {
            record_schema_stage(SchemaLifecycleStage::MigrationRequired);
            drop(shared);
            finish_required_migration(coordinator, database, backups, &refuse, now)
        }
        Err(error) => Err(error.into()),
    }
}

fn open_store_for_external_adoption(
    coordinator: &FileRuntimeCoordinator,
    data_dir: &Path,
    now: Timestamp,
    follows_schema_change: bool,
    startup_admission: &mut StartupAdmission,
    verified: VerifiedStartupInstallation<'_>,
) -> Result<(SqliteStore, FileSchemaLease), CliError> {
    let database = data_dir.join("proqi.sqlite3");
    let backups = data_dir.join("backups");
    let refuse = StoreConfig::new(
        database.clone(),
        backups.clone(),
        MigrationMode::Refuse,
        now,
    );
    let opened = match SqliteStore::open(&refuse) {
        Ok(store) => {
            if follows_schema_change {
                record_schema_stage(SchemaLifecycleStage::FollowerRevalidated);
            }
            store
        }
        Err(StoreError::MigrationRequired { .. }) => {
            record_schema_stage(SchemaLifecycleStage::MigrationRequired);
            record_schema_stage(SchemaLifecycleStage::MigrationStarted);
            let migrate = StoreConfig::new(database, backups, MigrationMode::Allow, now);
            let store = SqliteStore::open(&migrate)?;
            record_schema_stage(SchemaLifecycleStage::MigrationCompleted);
            store
        }
        Err(error) => return Err(error.into()),
    };
    verified.revalidate()?;
    startup_admission.finish_after_store_ready()?;
    drop(opened);
    let shared = coordinator.acquire_schema_shared()?;
    let store = SqliteStore::open(&refuse)?;
    Ok((store, shared))
}

fn finish_required_migration(
    coordinator: &FileRuntimeCoordinator,
    database: PathBuf,
    backups: PathBuf,
    refuse: &StoreConfig,
    now: Timestamp,
) -> Result<(SqliteStore, FileSchemaLease), CliError> {
    let exclusive = match coordinator.acquire_schema_exclusive() {
        Ok(exclusive) => exclusive,
        Err(crate::ports::runtime::RuntimeError::SchemaBusy) => {
            return revalidate_completed_migration(coordinator, refuse);
        }
        Err(error) => return Err(error.into()),
    };
    match SqliteStore::open(refuse) {
        Ok(current) => {
            drop(current);
            record_schema_stage(SchemaLifecycleStage::FollowerRevalidated);
            drop(exclusive);
            let shared = coordinator.acquire_schema_shared()?;
            let store = SqliteStore::open(refuse)?;
            return Ok((store, shared));
        }
        Err(StoreError::MigrationRequired { .. }) => {}
        Err(error) => return Err(error.into()),
    }
    record_schema_stage(SchemaLifecycleStage::MigrationStarted);
    let migrate = StoreConfig::new(database, backups, MigrationMode::Allow, now);
    let _revalidated = SqliteStore::open(&migrate)?;
    record_schema_stage(SchemaLifecycleStage::MigrationCompleted);
    drop(exclusive);
    let shared = coordinator.acquire_schema_shared()?;
    let store = SqliteStore::open(refuse)?;
    Ok((store, shared))
}

fn revalidate_completed_migration(
    coordinator: &FileRuntimeCoordinator,
    refuse: &StoreConfig,
) -> Result<(SqliteStore, FileSchemaLease), CliError> {
    let Some(shared) = coordinator.try_acquire_schema_shared()? else {
        return Err(crate::ports::runtime::RuntimeError::SchemaBusy.into());
    };
    match SqliteStore::open(refuse) {
        Ok(store) => {
            record_schema_stage(SchemaLifecycleStage::FollowerRevalidated);
            Ok((store, shared))
        }
        Err(StoreError::MigrationRequired { .. }) => {
            Err(crate::ports::runtime::RuntimeError::SchemaBusy.into())
        }
        Err(error) => Err(error.into()),
    }
}

fn record_schema_stage(stage: SchemaLifecycleStage) {
    crate::adapters::diagnostics::record(SafeEvent::SchemaLifecycle { stage });
}

#[cfg(test)]
mod tests;
