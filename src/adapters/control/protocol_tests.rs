use std::{os::unix::fs::DirBuilderExt as _, path::Path};

use crate::{
    adapters::memory::FakeIdGenerator,
    ports::{
        control::{
            CONTROL_PROTOCOL_VERSION, ControlClient, ControlError, ControlMutation, ControlRequest,
            ControlResult, MAX_CONTROL_MESSAGE_BYTES,
        },
        environment::IdGenerator,
        runtime::InstanceInfo,
    },
};

use super::{
    ControlServer, LocalControlClient,
    transport::{connect, read_response, write_request},
};

#[test]
fn server_negotiates_protocol_and_bounds_encoded_messages() {
    let temporary = tempfile::tempdir().expect("temporary endpoint");
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let mut request = request(&mut ids, "body");
    let endpoint = endpoint(temporary.path());
    let server = ControlServer::spawn(&endpoint).expect("control server");
    let owner = InstanceInfo {
        instance_id: ids.instance_id(),
        session_id: request.session_id,
        pid: std::process::id(),
        version: "test".to_owned(),
        storage_protocol: 1,
        control_protocol: Some(CONTROL_PROTOCOL_VERSION),
        control_endpoint: Some(endpoint.clone()),
        update: None,
        launch_directory: std::env::temp_dir()
            .join("proqi-control")
            .to_string_lossy()
            .into_owned(),
        started_at: crate::domain::Timestamp::from_millis(1),
    };
    request.protocol = CONTROL_PROTOCOL_VERSION + 1;
    let stream = connect(&endpoint, owner.pid).expect("protocol stream");
    write_request(&stream, &request).expect("protocol request");
    let response = read_response(&stream).expect("protocol response");
    assert!(matches!(
        response.result,
        ControlResult::Rejected { code, .. } if code == "protocol_mismatch"
    ));

    request.protocol = CONTROL_PROTOCOL_VERSION;
    let ControlMutation::Add { content, .. } = &mut request.mutation else {
        panic!("add request fixture");
    };
    *content = "x".repeat(MAX_CONTROL_MESSAGE_BYTES);
    assert!(matches!(
        LocalControlClient.send(&owner, &request),
        Err(ControlError::MessageTooLarge)
    ));
    server.stop().expect("server stop");
}

fn request(ids: &mut FakeIdGenerator, content: &str) -> ControlRequest {
    ControlRequest {
        protocol: CONTROL_PROTOCOL_VERSION,
        request_id: ids.request_id(),
        session_id: ids.session_id(),
        mutation: ControlMutation::Add {
            operation_id: ids.operation_id(),
            thought_id: ids.thought_id(),
            content: content.to_owned(),
            annotations: Vec::new(),
            position: None,
        },
    }
}

fn endpoint(directory: &Path) -> String {
    let private = directory.join("control");
    let mut builder = std::fs::DirBuilder::new();
    builder.mode(0o700);
    builder.create(&private).expect("private endpoint parent");
    private.join("protocol.sock").to_string_lossy().into_owned()
}
