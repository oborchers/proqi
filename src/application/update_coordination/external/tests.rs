use crate::{
    adapters::memory::FakeIdGenerator,
    domain::{InstallationIdentity, InstallationKind, StableVersion, Timestamp},
    ports::{
        control::CONTROL_PROTOCOL_VERSION,
        environment::IdGenerator as _,
        runtime::{InstanceInfo, UpdateInstanceContext},
        store::STORAGE_PROTOCOL_VERSION,
        update::UPDATE_CONTROL_PROTOCOL_VERSION,
    },
};

use super::{ExternalUpgradeBlockerReason, plan};

mod behavior;
mod capacity;

fn instance(
    ids: &mut FakeIdGenerator,
    version: &str,
    storage_protocol: u32,
    installation: InstallationIdentity,
    update_protocol: u32,
) -> InstanceInfo {
    InstanceInfo {
        instance_id: ids.instance_id(),
        session_id: ids.session_id(),
        pid: 42,
        version: version.to_owned(),
        storage_protocol,
        control_protocol: Some(CONTROL_PROTOCOL_VERSION),
        control_endpoint: Some("/private/proqi.sock".to_owned()),
        update: Some(UpdateInstanceContext {
            installation_identity: installation,
            protocol: update_protocol,
            replacement: None,
        }),
        launch_directory: "/private".to_owned(),
        started_at: Timestamp::from_millis(1),
    }
}

#[test]
fn published_protocol_two_owner_blocks_while_protocol_three_owner_can_quiesce() {
    let mut ids = FakeIdGenerator::new(1_800_000_000_000);
    let installation = InstallationIdentity::from_digest([41; 32]);
    let current = StableVersion::parse("0.10.1").expect("current");
    let published = instance(
        &mut ids,
        "0.10.0",
        STORAGE_PROTOCOL_VERSION,
        installation,
        2,
    );
    let compatible = instance(
        &mut ids,
        "0.10.0",
        STORAGE_PROTOCOL_VERSION,
        installation,
        UPDATE_CONTROL_PROTOCOL_VERSION,
    );

    let result = plan(
        vec![published.clone(), compatible.clone()],
        installation,
        InstallationKind::HomebrewFormula,
        &current,
    );

    assert_eq!(result.compatible_older, [compatible]);
    assert_eq!(result.blockers.len(), 1);
    assert_eq!(result.blockers[0].instance_id, published.instance_id);
    assert_eq!(
        result.blockers[0].reason,
        ExternalUpgradeBlockerReason::OlderIncompatible
    );
}

#[test]
fn same_version_requires_current_storage_and_newer_runtime_blocks() {
    let mut ids = FakeIdGenerator::new(1_800_100_000_000);
    let installation = InstallationIdentity::from_digest([42; 32]);
    let current = StableVersion::parse("0.10.0").expect("current");
    let incompatible = instance(
        &mut ids,
        "0.10.0",
        STORAGE_PROTOCOL_VERSION.saturating_sub(1),
        installation,
        UPDATE_CONTROL_PROTOCOL_VERSION,
    );
    let newer = instance(
        &mut ids,
        "0.11.0",
        STORAGE_PROTOCOL_VERSION,
        installation,
        UPDATE_CONTROL_PROTOCOL_VERSION,
    );

    let result = plan(
        vec![incompatible, newer],
        installation,
        InstallationKind::HomebrewFormula,
        &current,
    );

    assert!(result.compatible_older.is_empty());
    assert_eq!(result.blockers.len(), 2);
    assert_eq!(
        result.blockers[0].reason,
        ExternalUpgradeBlockerReason::IncompatibleWriter
    );
    assert_eq!(
        result.blockers[1].reason,
        ExternalUpgradeBlockerReason::NewerRuntime
    );
}

#[test]
fn source_installation_owner_is_a_restart_blocker_before_preparation() {
    let mut ids = FakeIdGenerator::new(1_800_200_000_000);
    let installation = InstallationIdentity::from_digest([43; 32]);
    let current = StableVersion::parse("0.11.0").expect("current");
    let owner = instance(
        &mut ids,
        "0.10.0",
        STORAGE_PROTOCOL_VERSION,
        installation,
        UPDATE_CONTROL_PROTOCOL_VERSION,
    );

    let result = plan(
        vec![owner.clone()],
        installation,
        InstallationKind::SourceOrUnknown,
        &current,
    );

    assert!(result.compatible_older.is_empty());
    assert_eq!(result.blockers.len(), 1);
    assert_eq!(result.blockers[0].instance_id, owner.instance_id);
    assert_eq!(
        result.blockers[0].reason,
        ExternalUpgradeBlockerReason::RestartUnsupported
    );
}
