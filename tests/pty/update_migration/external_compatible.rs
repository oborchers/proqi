//! Real automatic convergence for externally replaced compatible live owners.

use proqi::{
    adapters::update::FileUpdateStateStore,
    domain::{StableVersion, Timestamp},
    ports::update::{ReleaseObservation, UpdateStateStore as _},
};

use super::{
    Owners, active_instances,
    automatic::{
        assert_cross_version_diagnostics, assert_exact_replacements,
        assert_store_after_automatic_update, create_ambiguous_sessions, launch_plan,
        wait_for_exact_replacements,
    },
    control_ready, diagnostic_content, isolated_state,
    old_fixture::{OLD_VERSION, OldFixture},
};

const OWNER_COUNT: usize = 5;

#[test]
fn compatible_live_owners_automatically_converge_after_external_replacement() {
    let _fixture_guard = super::cross_version_fixture_guard();
    let fixture = OldFixture::build();
    let state = isolated_state("proqi-ext-compatible");
    let installation = fixture.installation(state.path());
    let old_binary = installation.old_binary.to_string_lossy().into_owned();
    let sessions = create_ambiguous_sessions(&old_binary, state.path(), OWNER_COUNT);
    let launches = launch_plan(state.path(), &sessions);
    let mut owners = Owners::spawn_in_directories(&old_binary, state.path(), &launches);
    owners.wait_ready(state.path());
    let before = active_instances(state.path());
    assert_eq!(before.len(), OWNER_COUNT);
    assert!(before.iter().all(|owner| owner.version == OLD_VERSION));
    assert!(before.iter().all(control_ready));
    assert!(before.iter().all(|owner| {
        owner.update.as_ref().is_some_and(|context| {
            context.installation_identity == installation.identity
                && context.protocol == proqi::ports::update::UPDATE_CONTROL_PROTOCOL_VERSION
        })
    }));
    seed_stale_cache(state.path(), installation.identity);
    let diagnostics = diagnostic_content(state.path());
    let migration_started_before = diagnostics
        .matches("\"stage\":\"migration_started\"")
        .count();
    let migration_completed_before = diagnostics
        .matches("\"stage\":\"migration_completed\"")
        .count();
    let follower_before = diagnostics
        .matches("\"stage\":\"follower_revalidated\"")
        .count();

    installation.replace_externally();
    let current_binary = installation.new_binary.to_string_lossy();
    let listed = super::json_command(&current_binary, state.path(), &["sessions", "list"]);
    assert_eq!(
        listed["data"]["sessions"]
            .as_array()
            .expect("session list")
            .len(),
        OWNER_COUNT
    );

    let after = wait_for_exact_replacements(state.path(), &before);
    assert_exact_replacements(&before, &after, &sessions);
    assert_store_after_automatic_update(state.path(), OWNER_COUNT);
    assert_cross_version_diagnostics(
        state.path(),
        OWNER_COUNT,
        migration_started_before,
        migration_completed_before,
        follower_before,
    );
    let cache = FileUpdateStateStore::new(&state.path().join("cache"))
        .expect("update cache")
        .load(installation.identity)
        .expect("reconciled cache");
    assert_eq!(
        cache.observed_installed_version,
        Some(StableVersion::parse(env!("CARGO_PKG_VERSION")).expect("current version"))
    );
    assert_eq!(
        cache.latest_stable,
        Some(StableVersion::parse(OLD_VERSION).expect("old version"))
    );
    assert!(!cache.restart_needed);
    assert!(cache.external_restart.is_none());
    owners.stop();
    assert!(active_instances(state.path()).is_empty());
}

fn seed_stale_cache(state: &std::path::Path, installation: proqi::domain::InstallationIdentity) {
    let store = FileUpdateStateStore::new(&state.join("cache")).expect("update cache");
    let old = StableVersion::parse(OLD_VERSION).expect("old version");
    store
        .record_success(
            installation,
            ReleaseObservation::Latest {
                version: old.clone(),
                etag: None,
            },
            old.clone(),
            Timestamp::from_millis(1_800_900_000_000),
        )
        .expect("old cache observation");
    store
        .record_restart_state(installation, old, true)
        .expect("stale restart state");
}
