//! Real old coordinator to new schema replacement acceptance cohorts.

use super::{
    FileUpdateStateStore, InstanceInfo, STORAGE_PROTOCOL_VERSION, SUPPORTED_SCHEMA_VERSION,
    active_instances, control_ready, diagnostic_content, isolated_state, old_fixture::OldFixture,
};
use proqi::ports::update::UpdateStateStore as _;
use rusqlite::Connection;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

#[test]
fn old_coordinator_converges_real_schema_changing_replacements() {
    let _fixture_guard = super::fixture_lock::acquire();
    let fixture = OldFixture::build();
    for count in [3, 15] {
        assert_automatic_schema_update(fixture, count);
    }
}

fn assert_automatic_schema_update(fixture: &OldFixture, count: usize) {
    let state = isolated_state("proqi-cross-version-update");
    let installation = fixture.installation(state.path());
    let sessions = create_ambiguous_sessions(
        &installation.old_binary.to_string_lossy(),
        state.path(),
        count,
    );
    let mut launch_order = launch_plan(state.path(), &sessions);
    launch_order.reverse();
    let mut owners = super::Owners::spawn_in_directories(
        &installation.old_binary.to_string_lossy(),
        state.path(),
        &launch_order,
    );
    let evidence = super::evidence::Evidence::new(state.path(), count);
    owners.wait_ready(state.path());
    let before = active_instances(state.path());
    assert_launch_directory_matrix(state.path(), &before, count);
    let initiating = before[count / 2].clone();
    let diagnostics_before = diagnostic_content(state.path());
    let migration_started_before = diagnostics_before
        .matches("\"stage\":\"migration_started\"")
        .count();
    let migration_completed_before = diagnostics_before
        .matches("\"stage\":\"migration_completed\"")
        .count();
    let follower_before = diagnostics_before
        .matches("\"stage\":\"follower_revalidated\"")
        .count();
    let converged_before = diagnostics_before
        .matches("\"stage\":\"board_ready\"")
        .count();

    evidence.stage("coordination");
    let execution = fixture.coordinate(
        state.path(),
        &installation,
        initiating.instance_id,
        initiating.session_id,
        &mut owners,
    );
    evidence.execution(&execution);
    evidence.stage("coordination_assertions");
    assert_execution(&execution, count, &evidence);

    evidence.stage("replacement_verification");
    let after = wait_for_exact_replacements(state.path(), &before);
    assert_exact_replacements(&before, &after, &sessions);
    assert_store_after_automatic_update(state.path(), count);
    assert_cross_version_diagnostics(
        state.path(),
        count,
        migration_started_before,
        migration_completed_before,
        follower_before,
    );
    evidence.stage("final_convergence");
    let cache = wait_for_final_convergence(
        state.path(),
        installation.identity,
        initiating.session_id,
        converged_before,
    );
    assert!(!cache.restart_needed);
    assert_eq!(
        cache
            .release_highlights
            .as_ref()
            .map(proqi::domain::ReleaseHighlightAnnouncement::session_id),
        Some(initiating.session_id)
    );
    evidence.stage("normal_shutdown");
    owners.stop();
    assert!(active_instances(state.path()).is_empty());
}

fn assert_execution(
    execution: &serde_json::Value,
    count: usize,
    evidence: &super::evidence::Evidence,
) {
    for field in [
        "prepared_participants",
        "quiescence_requests",
        "quiesced_participants",
        "restart_requests",
        "restart_accepted",
    ] {
        assert_eq!(execution[field], count, "{field}: {}", evidence.summary());
    }
    assert_eq!(
        execution["replacement_missing"],
        0,
        "{}",
        evidence.summary()
    );
    assert_eq!(
        execution["restart_failed"],
        serde_json::json!([]),
        "{}",
        evidence.summary()
    );
}

fn wait_for_final_convergence(
    state: &Path,
    installation: proqi::domain::InstallationIdentity,
    initiating_session: proqi::domain::SessionId,
    converged_before: usize,
) -> proqi::domain::UpdateCacheState {
    let update_state = FileUpdateStateStore::new(&state.join("cache")).expect("cache");
    let deadline = Instant::now() + super::OWNER_TIMEOUT;
    loop {
        let cache = update_state.load(installation).expect("update cache");
        let exact_highlight = cache
            .release_highlights
            .as_ref()
            .is_some_and(|announcement| announcement.session_id() == initiating_session);
        let converged = diagnostic_content(state)
            .matches("\"stage\":\"board_ready\"")
            .count()
            == converged_before + 1;
        if !cache.restart_needed && exact_highlight && converged {
            return cache;
        }
        assert!(
            Instant::now() < deadline,
            "initiating session {initiating_session} did not reach final convergence: {cache:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

pub(super) fn launch_plan(state: &Path, sessions: &[String]) -> Vec<(String, PathBuf)> {
    let shared = state.join("cwd-shared");
    let mixed_a = state.join("cwd-mixed-a");
    let mixed_b = state.join("cwd-mixed-b");
    for directory in [&shared, &mixed_a, &mixed_b] {
        fs::create_dir(directory).expect("launch directory");
    }
    sessions
        .iter()
        .enumerate()
        .map(|(index, session)| {
            let directory = if sessions.len() == 3 || index < 3 {
                &shared
            } else if index.is_multiple_of(2) {
                &mixed_a
            } else {
                &mixed_b
            };
            (session.clone(), directory.clone())
        })
        .collect()
}

fn assert_launch_directory_matrix(state: &Path, before: &[InstanceInfo], count: usize) {
    let counts = before.iter().fold(BTreeMap::new(), |mut counts, instance| {
        *counts
            .entry(instance.launch_directory.as_str())
            .or_insert(0) += 1;
        counts
    });
    let shared = state.join("cwd-shared").to_string_lossy().into_owned();
    assert_eq!(counts.get(shared.as_str()).copied(), Some(3));
    if count >= 15 {
        assert_eq!(counts.len(), 3, "large cohort must span mixed directories");
    } else {
        assert_eq!(counts.len(), 1, "incident cohort shares one exact cwd");
    }
}

pub(super) fn create_ambiguous_sessions(binary: &str, state: &Path, count: usize) -> Vec<String> {
    let sessions = (0..count)
        .map(|_| {
            let created = super::json_command(binary, state, &[]);
            let session = created["data"]["session_id"]
                .as_str()
                .expect("session ID")
                .to_owned();
            let added = super::json_input_command(
                binary,
                state,
                &["thoughts", "add", &session],
                "identical content for exact replacement",
            );
            assert_eq!(added["data"]["receipt"]["idempotent_replay"], false);
            session
        })
        .collect::<Vec<_>>();
    Connection::open(state.join("data/proqi.sqlite3"))
        .expect("database")
        .execute("UPDATE sessions SET name = 'duplicate'", [])
        .expect("duplicate names");
    sessions
}

pub(super) fn wait_for_exact_replacements(
    state: &Path,
    before: &[InstanceInfo],
) -> Vec<InstanceInfo> {
    let deadline = Instant::now() + super::OWNER_TIMEOUT;
    loop {
        let active = active_instances(state);
        let exact = active.len() == before.len()
            && before.iter().all(|old| {
                active.iter().any(|candidate| {
                    candidate.session_id == old.session_id
                        && candidate.instance_id != old.instance_id
                        && candidate.version == env!("CARGO_PKG_VERSION")
                        && control_ready(candidate)
                })
            });
        if exact {
            return active;
        }
        assert!(
            Instant::now() < deadline,
            "exact replacements did not become ready: {active:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

pub(super) fn assert_exact_replacements(
    before: &[InstanceInfo],
    after: &[InstanceInfo],
    sessions: &[String],
) {
    assert_eq!(after.len(), sessions.len());
    let expected = sessions.iter().cloned().collect::<BTreeSet<_>>();
    let restored = after
        .iter()
        .map(|instance| instance.session_id.to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(restored, expected);
    for old in before {
        let replacement = after
            .iter()
            .find(|candidate| candidate.session_id == old.session_id)
            .expect("exact session replacement");
        assert_ne!(replacement.instance_id, old.instance_id);
        assert_eq!(
            replacement.pid, old.pid,
            "Unix exec preserves the pane process"
        );
        assert_eq!(replacement.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(replacement.launch_directory, old.launch_directory);
        assert!(control_ready(replacement));
    }
}

pub(super) fn assert_store_after_automatic_update(state: &Path, count: usize) {
    let connection = Connection::open(state.join("data/proqi.sqlite3")).expect("database");
    let integrity: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .expect("integrity");
    assert_eq!(integrity, "ok");
    let versions: (u32, u32) = connection
        .query_row(
            "SELECT schema_version, storage_protocol FROM schema_meta",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("schema versions");
    assert_eq!(
        versions,
        (SUPPORTED_SCHEMA_VERSION, STORAGE_PROTOCOL_VERSION)
    );
    let migration_sixteen: i64 = connection
        .query_row(
            "SELECT count(*) FROM migration_history WHERE version = 16",
            [],
            |row| row.get(0),
        )
        .expect("migration history");
    assert_eq!(migration_sixteen, 1);
    let runtime_entries = fs::read_dir(state.join("runtime/instances"))
        .expect("runtime metadata")
        .count();
    assert_eq!(runtime_entries, count);
}

pub(super) fn assert_cross_version_diagnostics(
    state: &Path,
    count: usize,
    started_before: usize,
    completed_before: usize,
    follower_before: usize,
) {
    let content = diagnostic_content(state);
    assert_eq!(
        content.matches("\"stage\":\"migration_started\"").count(),
        started_before + 1
    );
    assert_eq!(
        content.matches("\"stage\":\"migration_completed\"").count(),
        completed_before + 1
    );
    let follower_revalidations = content
        .matches("\"stage\":\"follower_revalidated\"")
        .count()
        .saturating_sub(follower_before);
    assert_eq!(
        follower_revalidations,
        count.saturating_sub(1),
        "every non-migrating replacement must revalidate as a follower"
    );
    assert!(!content.contains("schema_busy"));
}
