//! Owner-protocol negotiation for forwarded mutations and read synchronization.

use super::{required_protocol, sync_protocol};
use crate::{
    cli::error_code::ErrorCode,
    domain::{ExportDisposition, InstanceId, OperationId, SessionId, ThoughtId, Timestamp},
    ports::{control::ControlMutation, runtime::InstanceInfo},
};

#[test]
fn export_completion_refuses_owners_older_than_protocol_thirteen() {
    let identity = "06g30t8fudrq55fdkjqr6mpe44";
    let owner = |control_protocol| InstanceInfo {
        instance_id: format!("ins_{identity}")
            .parse::<InstanceId>()
            .expect("instance"),
        session_id: format!("ses_{identity}")
            .parse::<SessionId>()
            .expect("session"),
        pid: 1,
        version: "0.14.0".to_owned(),
        storage_protocol: 20,
        control_protocol: Some(control_protocol),
        control_endpoint: None,
        update: None,
        launch_directory: "/work".to_owned(),
        started_at: Timestamp::from_millis(1),
    };
    let mutation = ControlMutation::ExportThoughts {
        operation_id: format!("op_{identity}")
            .parse::<OperationId>()
            .expect("operation"),
        thought_ids: vec![
            format!("tht_{identity}")
                .parse::<ThoughtId>()
                .expect("thought"),
        ],
        expected_digests: vec![[0; 32]],
        disposition: ExportDisposition::Remove,
        reference_thought_id: None,
        output_path: "/work/out.txt".to_owned(),
    };
    let refused = required_protocol(&owner(12), &mutation).expect_err("old owner");
    assert_eq!(refused.code(), ErrorCode::ProtocolMismatch);
    assert_eq!(
        required_protocol(&owner(13), &mutation).expect("current owner"),
        13
    );
}

#[test]
fn read_sync_degrades_for_legacy_owners_but_rejects_newer_protocols() {
    assert_eq!(
        sync_protocol(None).expect("unadvertised legacy owner"),
        None
    );
    assert_eq!(sync_protocol(Some(3)).expect("protocol three owner"), None);
    assert_eq!(
        sync_protocol(Some(4)).expect("protocol four owner"),
        Some(4)
    );
    assert_eq!(sync_protocol(Some(7)).expect("current owner"), Some(7));
    assert!(sync_protocol(Some(crate::ports::control::CONTROL_PROTOCOL_VERSION + 1)).is_err());
}
