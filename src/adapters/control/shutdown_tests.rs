//! Deterministic control admission and cancellation shutdown contracts.

use std::{
    io::Write as _,
    os::unix::fs::PermissionsExt as _,
    time::{Duration, Instant},
};

use crate::{
    adapters::{memory::FakeIdGenerator, process::CancellationFlag},
    domain::{OperationSequence, ThoughtId, Timestamp},
    ports::{
        control::{
            CONTROL_PROTOCOL_VERSION, ControlClient as _, ControlError, ControlMutation,
            ControlReceipt, ControlRequest, ControlResult,
        },
        environment::IdGenerator as _,
        runtime::InstanceInfo,
        store::{CommitReceipt, DurableIdentity},
    },
};

use super::{CancellableLocalControlClient, ControlServer, LocalControlClient, transport};

#[test]
fn shutdown_interrupts_a_partial_request_frame() {
    let temporary = tempfile::tempdir().expect("temporary endpoint");
    let endpoint = endpoint(temporary.path(), "partial-frame");
    let server = ControlServer::spawn(&endpoint).expect("control server");
    let stream = transport::connect(&endpoint, std::process::id()).expect("control stream");
    (&stream).write_all(b"{").expect("partial request");
    wait_for_active_stream(&server);

    let started = Instant::now();
    server
        .stop_before(started + Duration::from_millis(250))
        .expect("bounded server stop");
    assert!(started.elapsed() < Duration::from_millis(250));
}

#[test]
fn accepted_request_is_rejected_before_shutdown_completes() {
    let temporary = tempfile::tempdir().expect("temporary endpoint");
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let request = request(&mut ids, "shutdown race");
    let endpoint = endpoint(temporary.path(), "r");
    let server = ControlServer::spawn(&endpoint).expect("control server");
    let owner = owner(&mut ids, request.session_id, endpoint);
    std::thread::scope(|scope| {
        let client = scope.spawn(|| LocalControlClient.send(&owner, &request));
        let envelope = server
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("accepted request");
        server.request_stop();
        envelope.respond(ControlResult::Rejected {
            code: "owner_shutting_down".to_owned(),
            message: "active owner is shutting down".to_owned(),
        });
        assert!(matches!(
            client.join().expect("client thread"),
            Err(ControlError::Rejected { code, .. }) if code == "owner_shutting_down"
        ));
    });
    server.stop().expect("server stop");
}

#[test]
fn durable_receipt_in_flight_survives_owner_shutdown() {
    let temporary = tempfile::tempdir().expect("temporary endpoint");
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let request = request(&mut ids, "durable before shutdown");
    let expected = receipt(&request);
    let endpoint = endpoint(temporary.path(), "d");
    let server = ControlServer::spawn(&endpoint).expect("control server");
    let owner = owner(&mut ids, request.session_id, endpoint);
    std::thread::scope(|scope| {
        let client = scope.spawn(|| LocalControlClient.send(&owner, &request));
        let envelope = server
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("accepted request");
        server.request_stop();
        envelope.respond(ControlResult::Accepted(expected));
        assert_eq!(
            client
                .join()
                .expect("client thread")
                .expect("durable receipt"),
            expected
        );
    });
    server.stop().expect("server stop");
}

#[test]
fn runtime_cancellation_interrupts_an_unanswered_control_request() {
    let temporary = tempfile::tempdir().expect("temporary endpoint");
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let request = request(&mut ids, "blocked transfer");
    let endpoint = endpoint(temporary.path(), "c");
    let server = ControlServer::spawn(&endpoint).expect("control server");
    let owner = owner(&mut ids, request.session_id, endpoint);
    let cancellation = CancellationFlag::default();
    std::thread::scope(|scope| {
        let client_cancellation = cancellation.clone();
        let client = scope.spawn(|| {
            CancellableLocalControlClient::new(client_cancellation).send(&owner, &request)
        });
        let envelope = server
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("accepted request");
        cancellation.cancel();
        let started = Instant::now();
        assert!(matches!(
            client.join().expect("client thread"),
            Err(ControlError::Io(_))
        ));
        assert!(started.elapsed() < Duration::from_millis(250));
        drop(envelope);
    });
    server.stop().expect("server stop");
}

fn wait_for_active_stream(server: &ControlServer) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !server.has_active_stream() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        server.has_active_stream(),
        "server did not accept partial frame"
    );
}

fn request(ids: &mut FakeIdGenerator, content: &str) -> ControlRequest {
    let operation_id = ids.operation_id();
    ControlRequest {
        protocol: CONTROL_PROTOCOL_VERSION,
        request_id: ids.request_id(),
        session_id: ids.session_id(),
        mutation: ControlMutation::Add {
            operation_id,
            thought_id: ThoughtId::from_database_bytes(operation_id.database_bytes())
                .expect("thought ID"),
            content: content.to_owned(),
            annotations: Vec::new(),
            position: None,
        },
    }
}

fn receipt(request: &ControlRequest) -> ControlReceipt {
    ControlReceipt {
        thought_id: request.mutation.thought_id(),
        durable: CommitReceipt {
            session_id: request.session_id,
            sequence: OperationSequence::new(1),
            identity: DurableIdentity::Operation(
                request
                    .mutation
                    .durable_operation_id()
                    .expect("durable request"),
            ),
            idempotent_replay: false,
        },
    }
}

fn owner(
    ids: &mut FakeIdGenerator,
    session_id: crate::domain::SessionId,
    endpoint: String,
) -> InstanceInfo {
    InstanceInfo {
        instance_id: ids.instance_id(),
        session_id,
        pid: std::process::id(),
        started_at: Timestamp::from_millis(1_725_200_000_000),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        launch_directory: std::env::temp_dir().to_string_lossy().into_owned(),
        control_endpoint: Some(endpoint),
        control_protocol: Some(CONTROL_PROTOCOL_VERSION),
        storage_protocol: crate::ports::store::STORAGE_PROTOCOL_VERSION,
        update: None,
    }
}

fn endpoint(directory: &std::path::Path, suffix: &str) -> String {
    let parent = directory.join("c");
    std::fs::create_dir_all(&parent).expect("control parent");
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
        .expect("private parent");
    parent
        .join(format!("{suffix}.sock"))
        .to_string_lossy()
        .into_owned()
}
