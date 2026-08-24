use crate::{
    adapters::memory::FakeIdGenerator,
    domain::Timestamp,
    ports::{
        control::{
            CONTROL_PROTOCOL_VERSION, ControlClient, ControlError, ControlMutation, ControlRequest,
        },
        environment::IdGenerator,
        runtime::InstanceInfo,
    },
};

use super::{ControlServer, LocalControlClient};

#[test]
fn windows_control_is_unadvertised_and_has_no_pid_only_fallback() {
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let session_id = ids.session_id();
    let endpoint = r"\\.\pipe\proqi-test";
    let owner = InstanceInfo {
        instance_id: ids.instance_id(),
        session_id,
        pid: std::process::id(),
        version: "test".to_owned(),
        storage_protocol: 1,
        control_protocol: Some(CONTROL_PROTOCOL_VERSION),
        control_endpoint: Some(endpoint.to_owned()),
        launch_directory: r"C:\proqi-test".to_owned(),
        started_at: Timestamp::from_millis(1),
    };
    let request = ControlRequest {
        protocol: CONTROL_PROTOCOL_VERSION,
        request_id: ids.request_id(),
        session_id,
        mutation: ControlMutation::Delete {
            operation_id: ids.operation_id(),
            thought_id: ids.thought_id(),
        },
    };

    assert!(matches!(
        ControlServer::spawn(endpoint),
        Err(ControlError::Unsupported)
    ));
    assert!(matches!(
        LocalControlClient.send(&owner, &request),
        Err(ControlError::Unsupported)
    ));
}
