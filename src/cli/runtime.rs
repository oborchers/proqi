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
        update::SystemInstallDetector,
    },
    application::LeasedSession,
    domain::Timestamp,
    ports::{
        environment::{AppPaths, Clock, Environment, IdGenerator, Paths},
        runtime::RuntimeCoordinator,
        store::{MigrationMode, StoreError},
        update::{InstallDetector as _, UPDATE_CONTROL_PROTOCOL_VERSION},
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
    installation: Option<crate::domain::Installation>,
    schema_lease: FileSchemaLease,
}

impl RuntimeContext {
    pub(super) fn open(state_root: Option<&Path>) -> Result<Self, CliError> {
        let cwd = SystemEnvironment
            .current_directory()
            .map_err(|error| CliError::new("environment_failed", error.to_string(), 1))?;
        let paths = resolve_paths(state_root)?;
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
                coordinator
                    .clone()
                    .with_update_context(installation.identity, UPDATE_CONTROL_PROTOCOL_VERSION)
            });
        let (store, schema_lease) = open_store(&coordinator, &paths.data_dir, clock.now())?;
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
            installation,
            schema_lease,
        })
    }

    pub(super) fn terminal_settings(
        &self,
    ) -> Result<crate::ui::UiSettings, crate::adapters::terminal::TerminalError> {
        crate::adapters::terminal::load_settings(&self.config_dir)
    }

    pub(super) fn into_terminal(
        self,
        session: LeasedSession<crate::adapters::runtime::FileSessionLease>,
        settings: crate::ui::UiSettings,
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
        }
    }
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
    let exclusive = coordinator.acquire_schema_exclusive()?;
    let migrate = StoreConfig::new(database, backups, MigrationMode::Allow, now);
    let _revalidated = SqliteStore::open(&migrate)?;
    drop(exclusive);
    let shared = coordinator.acquire_schema_shared()?;
    let store = SqliteStore::open(refuse)?;
    Ok((store, shared))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rusqlite::Connection;

    use super::*;
    use crate::{adapters::memory::FakeIdGenerator, ports::store::SUPPORTED_SCHEMA_VERSION};

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
        .expect("contender coordinator");
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

        let (store, _shared) = finish_required_migration(
            &contender,
            database.clone(),
            backups.clone(),
            &refuse,
            Timestamp::from_millis(3),
        )
        .expect("stale contender revalidates");
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
