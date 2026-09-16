//! The production v0.9.0 to v0.10.0 external package replacement incident.

use std::{
    collections::BTreeSet,
    fs,
    os::unix::fs::symlink,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use proqi::{
    adapters::update::FileUpdateStateStore,
    domain::{StableVersion, Timestamp},
    ports::{
        store::{STORAGE_PROTOCOL_VERSION, SUPPORTED_SCHEMA_VERSION},
        update::{ReleaseObservation, UpdateStateStore as _},
    },
};
use rusqlite::Connection;

use super::{
    Owners, active_instances, control_ready, diagnostic_content,
    historical_fixture::{HistoricalFixture, HistoricalInstallation},
    isolated_state,
};

const OWNER_COUNT: usize = 5;

#[test]
fn external_homebrew_replacement_converges_real_v0_9_owners_without_refresh() {
    let _fixture_guard = super::cross_version_fixture_guard();
    let fixture = HistoricalFixture::build();
    let state = isolated_state("proqi-external-v0.9-convergence");
    let installation = fixture.install(state.path());
    let expected_content = create_historical_sessions(&installation, state.path());
    assert_historical_store(state.path());
    let launches = launch_plan(state.path(), expected_content.keys().cloned().collect());
    let historical_binary = installation.old_binary.to_string_lossy().into_owned();
    let mut owners = Owners::spawn_in_directories(&historical_binary, state.path(), &launches);
    wait_historical_ready(state.path(), &mut owners);
    let before = active_instances(state.path());
    assert_eq!(before.len(), OWNER_COUNT);
    assert!(before.iter().all(|owner| owner.version == "0.9.0"));
    assert!(before.iter().all(|owner| owner.storage_protocol == 14));
    seed_incident_cache(state.path(), &installation);

    installation.replace_externally();
    let blocked = run_json(
        &installation.active_binary,
        state.path(),
        &["sessions", "list"],
    );
    assert!(!blocked.0.success(), "live v0.9 owners must block startup");
    assert_exact_blockers(&blocked.1, &before);
    assert!(!blocked.1.to_string().contains("start 0.9.0"));
    assert!(!blocked.1.to_string().contains(&historical_binary));

    hide_historical_runtime_metadata(state.path(), &before);
    owners.assert_running();
    assert!(
        active_instances(state.path()).is_empty(),
        "the schema lease must remain authoritative when live v0.9 metadata is absent"
    );
    let unregistered = run_json(
        &installation.active_binary,
        state.path(),
        &["sessions", "list"],
    );
    assert!(
        !unregistered.0.success(),
        "an unregistered v0.9 schema owner must prevent external adoption"
    );
    assert_eq!(unregistered.1["error"]["code"], "schema_busy");
    assert_incident_cache_unchanged(state.path(), &installation);

    owners.stop();
    assert!(active_instances(state.path()).is_empty());
    reset_cohort_signals(state.path());
    let diagnostics_before = diagnostic_counts(state.path());
    let current_binary = installation.active_binary.to_string_lossy().into_owned();
    let mut replacements = Owners::spawn_in_directories(&current_binary, state.path(), &launches);
    replacements.wait_ready(state.path());
    let after = active_instances(state.path());
    assert_exact_sessions_and_directories(&after, &launches);
    assert_current_store(state.path(), &expected_content);
    assert_schema_winner_and_followers(state.path(), diagnostics_before);
    assert_reconciled_cache(state.path(), &installation);

    let idempotent = run_json(
        &installation.active_binary,
        state.path(),
        &["sessions", "list"],
    );
    assert!(idempotent.0.success(), "idempotent retry: {}", idempotent.1);
    assert_current_store(state.path(), &expected_content);
    assert_schema_winner_and_followers(state.path(), diagnostics_before);
    replacements.stop();
    assert!(active_instances(state.path()).is_empty());
    assert_active_executable_mismatch_fails_closed(state.path(), &installation);
}

fn hide_historical_runtime_metadata(state: &Path, owners: &[proqi::ports::runtime::InstanceInfo]) {
    for owner in owners {
        let metadata = state
            .join("runtime/instances")
            .join(format!("{}.json", owner.instance_id));
        assert!(
            metadata.is_file(),
            "missing live metadata: {}",
            metadata.display()
        );
        fs::remove_file(metadata).expect("hide live historical metadata");
    }
}

fn assert_incident_cache_unchanged(state: &Path, installation: &HistoricalInstallation) {
    let cache = FileUpdateStateStore::new(&state.join("cache"))
        .expect("update cache")
        .load(installation.identity)
        .expect("incident update state");
    assert_eq!(
        cache.observed_installed_version,
        Some(StableVersion::parse("0.9.0").expect("historical version"))
    );
    assert!(cache.restart_needed);
    assert!(cache.external_restart.is_none());
}

fn reset_cohort_signals(state: &Path) {
    for path in [state.join("cohort.done"), state.join("cohort.group")] {
        assert!(
            path.is_file(),
            "cohort signal is not a file: {}",
            path.display()
        );
        fs::remove_file(path).expect("remove completed cohort signal");
    }
}

fn wait_historical_ready(state: &Path, owners: &mut Owners) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let active = active_instances(state);
        let ready = active.len() == OWNER_COUNT
            && active.iter().all(|owner| {
                owner.version == "0.9.0"
                    && owner.control_protocol.is_some()
                    && owner
                        .control_endpoint
                        .as_deref()
                        .is_some_and(|endpoint| Path::new(endpoint).exists())
            });
        if ready {
            return;
        }
        owners.assert_running();
        assert!(
            Instant::now() < deadline,
            "historical owner cohort did not become ready: {active:#?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn create_historical_sessions(
    installation: &HistoricalInstallation,
    state: &Path,
) -> std::collections::BTreeMap<String, String> {
    let binary = installation.old_binary.to_string_lossy();
    let contents = [
        "duplicate exact content".to_owned(),
        "duplicate exact content".to_owned(),
        "Grüße 界\t\u{1b}[31m\u{7}\r\ncontrol heavy".to_owned(),
        format!("large durable content Grüße 界\n{}", "x".repeat(120 * 1024)),
        "same cwd, distinct exact session".to_owned(),
    ];
    let sessions = contents
        .into_iter()
        .map(|content| {
            let created = super::json_command(&binary, state, &[]);
            let session = created["data"]["session_id"]
                .as_str()
                .expect("historical session ID")
                .to_owned();
            let added =
                super::json_input_command(&binary, state, &["thoughts", "add", &session], &content);
            assert_eq!(added["data"]["receipt"]["idempotent_replay"], false);
            (session, content)
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    Connection::open(state.join("data/proqi.sqlite3"))
        .expect("historical database")
        .execute("UPDATE sessions SET name = 'duplicate'", [])
        .expect("duplicate historical names");
    sessions
}

fn launch_plan(state: &Path, sessions: Vec<String>) -> Vec<(String, std::path::PathBuf)> {
    let shared = state.join("cwd-shared");
    let mixed = state.join("cwd-mixed");
    fs::create_dir(&shared).expect("shared launch directory");
    fs::create_dir(&mixed).expect("mixed launch directory");
    sessions
        .into_iter()
        .enumerate()
        .map(|(index, session)| {
            let directory = if index < 3 { &shared } else { &mixed };
            (session, directory.clone())
        })
        .collect()
}

fn seed_incident_cache(state: &Path, installation: &HistoricalInstallation) {
    let store = FileUpdateStateStore::new(&state.join("cache")).expect("update cache");
    let historical = StableVersion::parse("0.9.0").expect("historical version");
    store
        .record_success(
            installation.identity,
            ReleaseObservation::Latest {
                version: historical.clone(),
                etag: None,
            },
            historical.clone(),
            Timestamp::from_millis(1_800_800_000_000),
        )
        .expect("historical cache observation");
    store
        .record_restart_state(installation.identity, historical, true)
        .expect("incident restart state");
}

fn run_json(
    binary: &Path,
    state: &Path,
    arguments: &[&str],
) -> (std::process::ExitStatus, serde_json::Value) {
    let output = Command::new(binary)
        .arg("--state-dir")
        .arg(state)
        .arg("--json")
        .args(arguments)
        .output()
        .expect("run JSON process");
    let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "JSON process output was invalid: {error}; stdout={}; stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status, value)
}

fn assert_exact_blockers(
    value: &serde_json::Value,
    before: &[proqi::ports::runtime::InstanceInfo],
) {
    assert_eq!(value["error"]["code"], "external_upgrade_blocked");
    let blockers = value["error"]["details"]["blockers"]
        .as_array()
        .expect("exact blocker details");
    let observed = blockers
        .iter()
        .map(|blocker| {
            (
                blocker["instance_id"].as_str().expect("blocker instance"),
                blocker["session_id"].as_str().expect("blocker session"),
                blocker["version"].as_str().expect("blocker version"),
                blocker["reason"].as_str().expect("blocker reason"),
            )
        })
        .collect::<BTreeSet<_>>();
    let expected = before
        .iter()
        .map(|owner| {
            (
                owner.instance_id.to_string(),
                owner.session_id.to_string(),
                owner.version.clone(),
                "older_incompatible".to_owned(),
            )
        })
        .collect::<BTreeSet<_>>();
    let observed = observed
        .into_iter()
        .map(|(instance, session, version, reason)| {
            (
                instance.to_owned(),
                session.to_owned(),
                version.to_owned(),
                reason.to_owned(),
            )
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(observed, expected);
}

fn assert_historical_store(state: &Path) {
    let connection = Connection::open(state.join("data/proqi.sqlite3")).expect("database");
    let versions: (u32, u32) = connection
        .query_row(
            "SELECT schema_version, storage_protocol FROM schema_meta",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("historical schema versions");
    assert_eq!(versions, (15, 14));
}

fn assert_exact_sessions_and_directories(
    active: &[proqi::ports::runtime::InstanceInfo],
    launches: &[(String, std::path::PathBuf)],
) {
    assert_eq!(active.len(), launches.len());
    for (session, directory) in launches {
        let owner = active
            .iter()
            .find(|owner| owner.session_id.to_string() == *session)
            .expect("exact restored session");
        assert_eq!(owner.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(owner.launch_directory, directory.to_string_lossy());
        assert!(control_ready(owner));
    }
}

fn assert_current_store(
    state: &Path,
    expected_content: &std::collections::BTreeMap<String, String>,
) {
    let connection = Connection::open(state.join("data/proqi.sqlite3")).expect("database");
    let integrity: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .expect("database integrity");
    assert_eq!(integrity, "ok");
    let versions: (u32, u32) = connection
        .query_row(
            "SELECT schema_version, storage_protocol FROM schema_meta",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("current schema versions");
    assert_eq!(
        versions,
        (SUPPORTED_SCHEMA_VERSION, STORAGE_PROTOCOL_VERSION)
    );
    let migration: i64 = connection
        .query_row(
            "SELECT count(*) FROM migration_history WHERE version = 16",
            [],
            |row| row.get(0),
        )
        .expect("migration winner");
    assert_eq!(migration, 1);
    let names: i64 = connection
        .query_row(
            "SELECT count(*) FROM sessions WHERE name = 'duplicate'",
            [],
            |row| row.get(0),
        )
        .expect("duplicate names");
    assert_eq!(
        names,
        i64::try_from(expected_content.len()).expect("session count")
    );
    let mut statement = connection
        .prepare("SELECT session_id, content FROM session_search")
        .expect("search content query");
    let stored = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .expect("search content rows")
        .collect::<Result<std::collections::BTreeMap<_, _>, _>>()
        .expect("stored exact content");
    assert_eq!(&stored, expected_content);
    for table in [
        "browser_operations",
        "browser_operation_receipts",
        "browser_history_receipts",
    ] {
        let count: i64 = connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("empty history table");
        assert_eq!(count, 0, "unexpected {table} record");
    }
    let history_state: i64 = connection
        .query_row("SELECT count(*) FROM browser_history_state", [], |row| {
            row.get(0)
        })
        .expect("browser history state");
    assert_eq!(history_state, 1);
    let runtime_records = fs::read_dir(state.join("runtime/instances"))
        .expect("runtime records")
        .count();
    assert_eq!(runtime_records, expected_content.len());
}

#[derive(Clone, Copy)]
struct DiagnosticCounts {
    required: usize,
    started: usize,
    completed: usize,
    followers: usize,
}

fn diagnostic_counts(state: &Path) -> DiagnosticCounts {
    let content = diagnostic_content(state);
    DiagnosticCounts {
        required: content.matches("\"stage\":\"migration_required\"").count(),
        started: content.matches("\"stage\":\"migration_started\"").count(),
        completed: content.matches("\"stage\":\"migration_completed\"").count(),
        followers: content
            .matches("\"stage\":\"follower_revalidated\"")
            .count(),
    }
}

fn assert_schema_winner_and_followers(state: &Path, before: DiagnosticCounts) {
    let after = diagnostic_counts(state);
    let required = after.required.saturating_sub(before.required);
    let followers = after.followers.saturating_sub(before.followers);
    assert!(
        required >= 1,
        "one starter must observe the required migration"
    );
    assert_eq!(after.started, before.started + 1);
    assert_eq!(after.completed, before.completed + 1);
    assert_eq!(followers + 1, required);
}

fn assert_reconciled_cache(state: &Path, installation: &HistoricalInstallation) {
    let cache = FileUpdateStateStore::new(&state.join("cache"))
        .expect("update cache")
        .load(installation.identity)
        .expect("reconciled update state");
    assert_eq!(
        cache.observed_installed_version,
        Some(StableVersion::parse(env!("CARGO_PKG_VERSION")).expect("current version"))
    );
    assert_eq!(
        cache.latest_stable,
        Some(StableVersion::parse("0.9.0").expect("stale latest evidence"))
    );
    assert!(!cache.restart_needed);
}

fn assert_active_executable_mismatch_fails_closed(
    state: &Path,
    installation: &HistoricalInstallation,
) {
    let alternate = installation
        .current_binary
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("formula root")
        .join("0.10.0-alternate/bin/proqi");
    fs::create_dir_all(alternate.parent().expect("alternate bin")).expect("alternate keg");
    fs::copy(&installation.current_binary, &alternate).expect("alternate executable");
    fs::write(
        alternate
            .parent()
            .and_then(Path::parent)
            .expect("alternate keg root")
            .join("INSTALL_RECEIPT.json"),
        b"{}",
    )
    .expect("alternate receipt");
    fs::remove_file(&installation.active_binary).expect("remove current active link");
    symlink(&alternate, &installation.active_binary).expect("activate mismatched executable");
    let mismatch = run_json(&installation.current_binary, state, &["sessions", "list"]);
    assert!(!mismatch.0.success());
    assert_eq!(mismatch.1["error"]["code"], "installation_failed");
}
