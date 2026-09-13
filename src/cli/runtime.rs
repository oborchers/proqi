//! Platform composition for paths, schema coordination, and SQLite.

use std::path::{Path, PathBuf};

use crate::{
    adapters::{
        runtime::{
            FileRuntimeCoordinator, FileSchemaLease, NativePaths, SystemClock, SystemEnvironment,
            SystemIdGenerator,
        },
        sqlite::{SqliteStore, StoreConfig},
        terminal::TerminalResources,
        update::{FileUpdateStateStore, SystemInstallDetector},
    },
    application::LeasedSession,
    domain::{InstallationIdentity, SessionId, StableVersion, Timestamp},
    ports::{
        environment::{AppPaths, Clock, Environment, IdGenerator, Paths},
        runtime::RuntimeCoordinator,
        store::{MigrationMode, StoreError},
        update::{
            InstallDetector as _, UPDATE_CONTROL_PROTOCOL_VERSION, UpdateLease,
            UpdateStateStore as _,
        },
    },
};

use super::output::CliError;

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
    startup_admission: Option<Box<dyn UpdateLease>>,
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
        let executable = SystemEnvironment
            .current_executable()
            .map_err(|error| CliError::new("environment_failed", error.to_string(), 1))?;
        prepare_state_paths(&paths, state_root)?;
        let clock = SystemClock;
        let mut ids = SystemIdGenerator;
        let instance_id = ids.instance_id();
        crate::adapters::diagnostics::initialize(&paths.data_dir, instance_id)
            .map_err(|error| CliError::new("diagnostics_failed", error.to_string(), 1))?;
        crate::adapters::diagnostics::record(
            crate::adapters::diagnostics::SafeEvent::RuntimeOpening { instance_id },
        );
        let config_dir = paths.config_dir.clone();
        let recovery_dir = paths.data_dir.join("recovery");
        let attachment_dir = paths.data_dir.join("attachments");
        let cache_dir = paths.cache_dir.clone();
        let state_root = state_root.map(Path::to_path_buf);
        let installation = SystemInstallDetector::current().detect().ok();
        let startup_admission = installation
            .as_ref()
            .map(|installation| {
                admit_update_start(
                    &cache_dir,
                    installation.identity,
                    exact_resume,
                    &StableVersion::parse(env!("CARGO_PKG_VERSION")).map_err(|error| {
                        CliError::new("invalid_build_version", error.to_string(), 1)
                    })?,
                )
            })
            .transpose()?;
        let coordinator = FileRuntimeCoordinator::new(
            paths.runtime_dir,
            instance_id,
            cwd.clone(),
            clock.now(),
            env!("CARGO_PKG_VERSION"),
        )?;
        let coordinator = installation
            .as_ref()
            .map_or(coordinator.clone(), |installation| {
                coordinator.clone().with_update_context(
                    installation.identity,
                    UPDATE_CONTROL_PROTOCOL_VERSION,
                    crate::adapters::process::replacement_context(),
                )
            });
        let (store, schema_lease) = open_store(&coordinator, &paths.data_dir, clock.now())?;
        crate::adapters::diagnostics::record(
            crate::adapters::diagnostics::SafeEvent::SchemaLifecycle {
                stage: crate::adapters::diagnostics::SchemaLifecycleStage::Ready,
            },
        );
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
            state_root,
            executable,
            installation,
            schema_lease,
            startup_admission: startup_admission.flatten(),
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
            startup_admission: self.startup_admission,
        }
    }
}

fn admit_update_start(
    cache_dir: &Path,
    installation: InstallationIdentity,
    exact_resume: Option<SessionId>,
    current: &StableVersion,
) -> Result<Option<Box<dyn UpdateLease>>, CliError> {
    let state = FileUpdateStateStore::new(cache_dir)
        .map_err(|error| CliError::new("update_state_failed", error.to_string(), 1))?;
    let lease = state
        .try_startup_lock(installation)
        .map_err(|error| CliError::new("update_state_failed", error.to_string(), 1))?;
    let cache = state
        .load(installation)
        .map_err(|error| CliError::new("update_state_failed", error.to_string(), 1))?;
    let target_matches = cache.observed_installed_version.as_ref() == Some(current);
    let obsolete_after_update = cache.restart_needed && !target_matches;
    if obsolete_after_update || lease.is_none() {
        let recovery = exact_resume.map_or_else(
            || "start the active Proqi executable and resume by exact SessionId".to_owned(),
            |session_id| {
                format!(
                    "start the active Proqi executable and run `proqi -r {session_id}` with the same state root"
                )
            },
        );
        return Err(CliError::new(
            "update_convergence_active",
            format!(
                "this Proqi executable cannot open the schema during update convergence; {recovery}"
            ),
            1,
        ));
    }
    Ok(lease)
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
) -> Result<(SqliteStore, FileSchemaLease), CliError> {
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
        Ok(store) => Ok((store, shared)),
        Err(StoreError::MigrationRequired { .. }) => {
            crate::adapters::diagnostics::record(
                crate::adapters::diagnostics::SafeEvent::SchemaLifecycle {
                    stage: crate::adapters::diagnostics::SchemaLifecycleStage::MigrationRequired,
                },
            );
            drop(shared);
            finish_required_migration(coordinator, database, backups, &refuse, now)
        }
        Err(error) => Err(error.into()),
    }
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
            crate::adapters::diagnostics::record(
                crate::adapters::diagnostics::SafeEvent::SchemaLifecycle {
                    stage: crate::adapters::diagnostics::SchemaLifecycleStage::FollowerRevalidated,
                },
            );
            drop(exclusive);
            let shared = coordinator.acquire_schema_shared()?;
            let store = SqliteStore::open(refuse)?;
            return Ok((store, shared));
        }
        Err(StoreError::MigrationRequired { .. }) => {}
        Err(error) => return Err(error.into()),
    }
    crate::adapters::diagnostics::record(
        crate::adapters::diagnostics::SafeEvent::SchemaLifecycle {
            stage: crate::adapters::diagnostics::SchemaLifecycleStage::MigrationStarted,
        },
    );
    let migrate = StoreConfig::new(database, backups, MigrationMode::Allow, now);
    let _revalidated = SqliteStore::open(&migrate)?;
    crate::adapters::diagnostics::record(
        crate::adapters::diagnostics::SafeEvent::SchemaLifecycle {
            stage: crate::adapters::diagnostics::SchemaLifecycleStage::MigrationCompleted,
        },
    );
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
            crate::adapters::diagnostics::record(
                crate::adapters::diagnostics::SafeEvent::SchemaLifecycle {
                    stage: crate::adapters::diagnostics::SchemaLifecycleStage::FollowerRevalidated,
                },
            );
            Ok((store, shared))
        }
        Err(StoreError::MigrationRequired { .. }) => {
            Err(crate::ports::runtime::RuntimeError::SchemaBusy.into())
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rusqlite::Connection;

    use super::*;
    use crate::{
        adapters::{memory::FakeIdGenerator, runtime::SchemaLockPolicy},
        ports::{store::SUPPORTED_SCHEMA_VERSION, update::UpdateLockKind},
    };

    #[test]
    fn update_start_admission_closes_obsolete_schema_entry_and_allows_exact_target() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let cache = temporary.path().join("cache");
        let installation = InstallationIdentity::from_digest([91; 32]);
        let old = StableVersion::parse("0.8.99").expect("old version");
        let target = StableVersion::parse("0.9.0").expect("target version");
        let mut ids = FakeIdGenerator::new(1_800_000_000_000);
        let session_id = ids.session_id();
        let state = FileUpdateStateStore::new(&cache).expect("update state");
        let convergence = state
            .try_lock(installation, UpdateLockKind::Convergence)
            .expect("installer lock")
            .expect("installer lease");

        let Err(blocked) = admit_update_start(&cache, installation, Some(session_id), &old) else {
            panic!("obsolete start entered during convergence");
        };
        let blocked = format!("{blocked:?}");
        assert!(blocked.contains("update_convergence_active"));
        assert!(blocked.contains(&session_id.to_string()));

        state
            .record_restart_state(installation, target.clone(), true)
            .expect("record target");
        assert!(
            admit_update_start(&cache, installation, Some(session_id), &target).is_err(),
            "target waits until every old schema lease is quiescent"
        );
        drop(convergence);
        assert!(
            admit_update_start(&cache, installation, Some(session_id), &old).is_err(),
            "obsolete executable stays barred after coordinator loss"
        );
        assert!(
            admit_update_start(&cache, installation, Some(session_id), &target)
                .expect("target start owns admission")
                .is_some()
        );
    }

    #[test]
    fn stale_migration_contender_revalidates_after_another_process_wins() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let data = temporary.path().join("data");
        let backups = data.join("backups");
        fs::create_dir(&data).expect("data directory");
        let database = data.join("proqi.sqlite3");
        Connection::open(&database)
            .expect("legacy database")
            .execute_batch("CREATE TABLE legacy(value TEXT); INSERT INTO legacy VALUES ('keep');")
            .expect("legacy fixture");
        let refuse = StoreConfig::new(
            database.clone(),
            backups.clone(),
            MigrationMode::Refuse,
            Timestamp::from_millis(1),
        );
        assert!(matches!(
            SqliteStore::open(&refuse),
            Err(StoreError::MigrationRequired { .. })
        ));

        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let runtime = temporary.path().join("runtime");
        let winner = FileRuntimeCoordinator::new(
            runtime.clone(),
            ids.instance_id(),
            temporary.path().to_path_buf(),
            Timestamp::from_millis(1),
            "winner",
        )
        .expect("winner coordinator");
        let contender = FileRuntimeCoordinator::new(
            runtime,
            ids.instance_id(),
            temporary.path().to_path_buf(),
            Timestamp::from_millis(2),
            "contender",
        )
        .expect("contender coordinator")
        .with_schema_lock_policy(
            SchemaLockPolicy::new(
                std::time::Duration::from_millis(40),
                std::time::Duration::from_millis(2),
            )
            .expect("bounded schema policy"),
        );
        let winner_lease = winner.acquire_schema_exclusive().expect("winner lease");
        drop(
            SqliteStore::open(&StoreConfig::new(
                database.clone(),
                backups.clone(),
                MigrationMode::Allow,
                Timestamp::from_millis(2),
            ))
            .expect("winner migration"),
        );
        drop(winner_lease);
        let follower_shared = winner
            .acquire_schema_shared()
            .expect("follower shared lease");

        let (store, _shared) = finish_required_migration(
            &contender,
            database.clone(),
            backups.clone(),
            &refuse,
            Timestamp::from_millis(3),
        )
        .expect("stale contender revalidates");
        drop(follower_shared);
        store.quick_check().expect("migrated integrity");
        let connection = Connection::open(database).expect("verify database");
        let schema: u32 = connection
            .query_row("SELECT schema_version FROM schema_meta", [], |row| {
                row.get(0)
            })
            .expect("schema version");
        let legacy: String = connection
            .query_row("SELECT value FROM legacy", [], |row| row.get(0))
            .expect("legacy value");
        assert_eq!(schema, SUPPORTED_SCHEMA_VERSION);
        assert_eq!(legacy, "keep");
        assert_eq!(fs::read_dir(backups).expect("backups").count(), 1);
    }
}
