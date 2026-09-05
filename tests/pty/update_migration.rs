//! Real schema-changing replacement cohorts and incomplete restart recovery.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use super::support::{json_command, json_input_command};
use proqi::{
    adapters::{
        control::LocalUpdateControlClient,
        runtime::{FileRuntimeCoordinator, SystemClock, SystemIdGenerator},
        update::{FileUpdateStateStore, SystemInstallDetector},
    },
    application::UpdateRestartCoordinator,
    domain::{InstanceId, RequestId, StableVersion, Timestamp},
    ports::{
        environment::{Clock as _, IdGenerator as _},
        runtime::{InstanceInfo, RuntimeCoordinator as _},
        store::{STORAGE_PROTOCOL_VERSION, SUPPORTED_SCHEMA_VERSION},
        update::{
            HomebrewInstaller, InstallDetector as _, UPDATE_CONTROL_PROTOCOL_VERSION, UpdateError,
            UpdateParticipantGateway, UpdatePrepareReply, UpdatePrepareRequest, UpdateRestartReply,
            UpdateRestartRequest, UpdateStateStore as _,
        },
    },
};
use rusqlite::Connection;

#[path = "update_migration/cohort.rs"]
mod cohort;

use cohort::{OWNER_TIMEOUT, Owners, active_instances, control_ready};

const INCIDENT_COHORT: usize = 21;
const STRESS_COHORT: usize = 26;
const FORWARDED_CONTENT: &str = "forwarded after follower convergence Grüße 界";
struct FakeInstaller;

impl HomebrewInstaller for FakeInstaller {
    fn upgrade(&mut self, expected: &StableVersion) -> Result<StableVersion, UpdateError> {
        Ok(expected.clone())
    }
}

struct RejectPeer {
    inner: LocalUpdateControlClient<SystemIdGenerator>,
    rejected: InstanceId,
}

impl UpdateParticipantGateway for RejectPeer {
    fn prepare(
        &mut self,
        participant: &InstanceInfo,
        request: &UpdatePrepareRequest,
    ) -> Result<UpdatePrepareReply, UpdateError> {
        self.inner.prepare(participant, request)
    }

    fn release(
        &mut self,
        participant: &InstanceInfo,
        operation_id: RequestId,
    ) -> Result<(), UpdateError> {
        self.inner.release(participant, operation_id)
    }

    fn restart(
        &mut self,
        participant: &InstanceInfo,
        request: &UpdateRestartRequest,
    ) -> Result<UpdateRestartReply, UpdateError> {
        if participant.instance_id == self.rejected {
            return Ok(UpdateRestartReply {
                instance_id: participant.instance_id,
                accepted: false,
            });
        }
        self.inner.restart(participant, request)
    }
}

#[test]
fn reported_and_stress_schema_changing_cohorts_restore_exact_ready_sessions() {
    for count in [INCIDENT_COHORT, STRESS_COHORT] {
        assert_schema_changing_cohort(count);
    }
}

fn assert_schema_changing_cohort(count: usize) {
    let state = isolated_state("proqi-migration-cohort");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let sessions = create_sessions(binary, state.path(), count);
    downgrade_to_schema_eleven(state.path());

    let mut ids = SystemIdGenerator;
    let barrier_coordinator = FileRuntimeCoordinator::new(
        state.path().join("runtime"),
        ids.instance_id(),
        state.path().to_path_buf(),
        SystemClock.now(),
        "schema-eleven-test-barrier",
    )
    .expect("schema barrier coordinator");
    let barrier = barrier_coordinator
        .acquire_schema_shared()
        .expect("schema eleven shared barrier");

    let mut owners = Owners::spawn(binary, state.path(), &sessions);
    owners.wait_started();
    wait_for_schema_stage(state.path(), count, &mut owners);
    drop(barrier);
    owners.wait_ready(state.path());
    let active = active_instances(state.path());
    let restored = active
        .iter()
        .map(|instance| instance.session_id.to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(restored, sessions.iter().cloned().collect());
    assert_eq!(active.len(), count);
    assert!(active.iter().all(control_ready));

    let forwarded = json_input_command(
        binary,
        state.path(),
        &["thoughts", "add", &sessions[0]],
        FORWARDED_CONTENT,
    );
    assert_eq!(forwarded["data"]["receipt"]["idempotent_replay"], false);
    assert_store(state.path(), &sessions);
    assert_diagnostics(state.path(), count);

    owners.stop();
    assert!(active_instances(state.path()).is_empty());
}

#[test]
fn rejected_peer_restart_releases_and_preserves_the_real_initiator() {
    let state = isolated_state("proqi-partial-restart");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let sessions = create_sessions(binary, state.path(), 2);
    let mut owners = Owners::spawn(binary, state.path(), &sessions);
    owners.wait_ready(state.path());
    rewrite_versions(state.path(), "0.5.0");
    let participants = active_instances(state.path());
    let initiating = participants[0].clone();
    let peer = participants[1].clone();
    let installation = SystemInstallDetector::for_executable(binary.into())
        .detect()
        .expect("installation identity");
    let registry = registry(state.path(), installation.identity);
    let update_state = FileUpdateStateStore::new(&state.path().join("cache")).expect("cache");
    let mut gateway = RejectPeer {
        inner: LocalUpdateControlClient::new(SystemIdGenerator),
        rejected: peer.instance_id,
    };
    let mut installer = FakeInstaller;
    let target = StableVersion::parse(env!("CARGO_PKG_VERSION")).expect("target");
    let deadline = Timestamp::from_millis(SystemClock.now().as_millis().saturating_add(10_000));
    let mut ids = SystemIdGenerator;

    let result =
        UpdateRestartCoordinator::new(&update_state, &registry, &mut gateway, &mut installer)
            .execute(
                ids.request_id(),
                initiating.instance_id,
                installation.identity,
                &target,
                deadline,
                &(),
            )
            .expect("partial restart result");

    assert_eq!(result.restart_requests, 1);
    assert_eq!(result.restart_accepted, 0);
    assert!(result.restart_failed.contains(&peer.instance_id));
    assert!(result.restart_failed.contains(&initiating.instance_id));
    assert!(
        update_state
            .load(installation.identity)
            .expect("cache")
            .restart_needed
    );
    assert!(
        update_state
            .load(installation.identity)
            .expect("cache")
            .release_highlights
            .is_none()
    );
    let still_active = active_instances(state.path());
    assert!(still_active.iter().any(|instance| {
        instance.instance_id == initiating.instance_id && control_ready(instance)
    }));
    let usable = json_input_command(
        binary,
        state.path(),
        &["thoughts", "add", &initiating.session_id.to_string()],
        "initiator stayed usable after incomplete convergence",
    );
    assert_eq!(usable["data"]["receipt"]["idempotent_replay"], false);
    owners.stop();
}

fn create_sessions(binary: &str, state: &Path, count: usize) -> Vec<String> {
    (0..count)
        .map(|index| {
            let created = json_command(binary, state, &[]);
            let session = created["data"]["session_id"]
                .as_str()
                .expect("session ID")
                .to_owned();
            let content = session_content(index);
            let added = json_input_command(binary, state, &["thoughts", "add", &session], &content);
            assert_eq!(added["data"]["receipt"]["idempotent_replay"], false);
            session
        })
        .collect()
}

fn session_content(index: usize) -> String {
    format!("replacement {index}: Grüße 界\t\u{1b}[31m\u{7}\r\nline two")
}

fn isolated_state(prefix: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in("/private/tmp")
        .expect("isolated state")
}

fn downgrade_to_schema_eleven(state: &Path) {
    Connection::open(state.join("data/proqi.sqlite3"))
        .expect("database")
        .execute_batch(
            "DELETE FROM migration_history WHERE version IN (12, 13);
             UPDATE schema_meta SET schema_version = 11, storage_protocol = 10;",
        )
        .expect("schema eleven fixture");
}

fn registry(
    state: &Path,
    installation: proqi::domain::InstallationIdentity,
) -> FileRuntimeCoordinator {
    let mut ids = SystemIdGenerator;
    FileRuntimeCoordinator::new(
        state.join("runtime"),
        ids.instance_id(),
        std::env::current_dir().expect("working directory"),
        SystemClock.now(),
        env!("CARGO_PKG_VERSION"),
    )
    .expect("registry")
    .with_update_context(installation, UPDATE_CONTROL_PROTOCOL_VERSION)
}

fn rewrite_versions(state: &Path, version: &str) {
    for entry in fs::read_dir(state.join("runtime/instances")).expect("runtime metadata") {
        let path = entry.expect("metadata entry").path();
        let mut info: InstanceInfo =
            serde_json::from_slice(&fs::read(&path).expect("metadata")).expect("instance");
        version.clone_into(&mut info.version);
        fs::write(path, serde_json::to_vec(&info).expect("instance JSON"))
            .expect("rewrite version");
    }
}

fn assert_store(state: &Path, sessions: &[String]) {
    let connection = Connection::open(state.join("data/proqi.sqlite3")).expect("database");
    let integrity: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .expect("integrity");
    let versions: (u32, u32) = connection
        .query_row(
            "SELECT schema_version, storage_protocol FROM schema_meta",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("schema versions");
    assert_eq!(integrity, "ok");
    assert_eq!(
        versions,
        (SUPPORTED_SCHEMA_VERSION, STORAGE_PROTOCOL_VERSION)
    );
    let migration_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM migration_history WHERE version IN (12, 13)",
            [],
            |row| row.get(0),
        )
        .expect("migration history count");
    assert_eq!(migration_rows, 2);
    for (table, expected) in [
        ("sessions", sessions.len()),
        ("session_search", sessions.len()),
        ("thoughts", sessions.len() + 1),
        ("board_operations", sessions.len() + 1),
    ] {
        let count: i64 = connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("durable count");
        assert_eq!(
            count,
            i64::try_from(expected).expect("bounded durable count"),
            "unexpected {table} count"
        );
    }
    assert_exact_content_and_search(&connection, sessions);
    let backups = fs::read_dir(state.join("data/backups"))
        .expect("migration backups")
        .count();
    assert_eq!(backups, 1);
}

fn assert_exact_content_and_search(connection: &Connection, sessions: &[String]) {
    let mut thought_statement = connection
        .prepare("SELECT content FROM thoughts WHERE deleted_at IS NULL")
        .expect("thought content query");
    let stored_thoughts = thought_statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("thought content rows")
        .collect::<Result<BTreeSet<_>, _>>()
        .expect("thought content");
    let mut expected_thoughts = (0..sessions.len())
        .map(session_content)
        .collect::<BTreeSet<_>>();
    assert!(expected_thoughts.insert(FORWARDED_CONTENT.to_owned()));
    assert_eq!(stored_thoughts, expected_thoughts);

    let mut search_statement = connection
        .prepare("SELECT session_id, content FROM session_search")
        .expect("search content query");
    let stored_search = search_statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .expect("search content rows")
        .collect::<Result<BTreeMap<_, _>, _>>()
        .expect("search content");
    let expected_search = sessions
        .iter()
        .enumerate()
        .map(|(index, session)| {
            let mut content = session_content(index);
            if index == 0 {
                content.push('\n');
                content.push_str(FORWARDED_CONTENT);
            }
            (session.clone(), content)
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(stored_search, expected_search);
    let matched: i64 = connection
        .query_row(
            "SELECT count(*) FROM session_search
             WHERE session_search MATCH '\"forwarded\" \"follower\" \"convergence\"'",
            [],
            |row| row.get(0),
        )
        .expect("FTS match");
    assert_eq!(matched, 1);
}

fn assert_diagnostics(state: &Path, ready: usize) {
    let content = diagnostic_content(state);
    assert!(content.contains("follower_revalidated"));
    assert!(content.matches("\"event\":\"runtime_ready\"").count() >= ready);
    assert!(!content.contains("schema_busy"));
}

fn wait_for_schema_stage(state: &Path, expected: usize, owners: &mut Owners) {
    let deadline = Instant::now() + OWNER_TIMEOUT;
    loop {
        let observed = diagnostic_content(state)
            .matches("\"stage\":\"migration_required\"")
            .count();
        if observed >= expected {
            return;
        }
        owners.assert_running();
        assert!(
            Instant::now() < deadline,
            "only {observed} of {expected} replacements reached MigrationRequired"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn diagnostic_content(state: &Path) -> String {
    let Ok(entries) = fs::read_dir(state.join("data/diagnostics")) else {
        return String::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .collect()
}
