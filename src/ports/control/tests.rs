use crate::{
    adapters::memory::FakeIdGenerator,
    domain::{
        ContentAnnotation, ContentAnnotationKind, InstallationIdentity, StableVersion, Timestamp,
    },
    ports::environment::IdGenerator,
    ports::update::{UpdatePrepareReply, UpdatePrepareRequest},
};

use super::{
    CAPTURE_CONTROL_PROTOCOL_VERSION, CONTROL_PROTOCOL_VERSION, ControlCaptureReceipt,
    ControlMutation, ControlRequest, ControlResponse, ControlResult, ControlUpdateReceipt,
};

#[test]
fn plain_and_legacy_annotations_keep_their_existing_minimum_protocols() {
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let plain = ControlRequest {
        protocol: 1,
        request_id: ids.request_id(),
        session_id: ids.session_id(),
        mutation: ControlMutation::Add {
            operation_id: ids.operation_id(),
            thought_id: ids.thought_id(),
            content: "plain".to_owned(),
            annotations: Vec::new(),
            position: None,
        },
    };
    let encoded = serde_json::to_string(&plain).expect("serialize v1 request");
    assert!(!encoded.contains("annotations"));
    let decoded: ControlRequest = serde_json::from_str(&encoded).expect("deserialize v1");
    assert_eq!(decoded, plain);
    assert!(!decoded.mutation.requires_protocol_two());

    let annotated = ControlMutation::Add {
        operation_id: ids.operation_id(),
        thought_id: ids.thought_id(),
        content: "/tmp/a.png".to_owned(),
        annotations: vec![ContentAnnotation {
            start: 0,
            end: 10,
            kind: ContentAnnotationKind::Attachment {
                image: true,
                display_name: "a.png".to_owned(),
            },
        }],
        position: None,
    };
    assert!(annotated.requires_protocol_two());
    assert_eq!(annotated.minimum_protocol(), 2);
}

#[test]
fn invocation_reference_annotations_require_protocol_six() {
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let mutation = ControlMutation::Add {
        operation_id: ids.operation_id(),
        thought_id: ids.thought_id(),
        content: "Herdr collaborator: reviewer".to_owned(),
        annotations: vec![ContentAnnotation {
            start: 0,
            end: 28,
            kind: ContentAnnotationKind::InvocationReference {
                display_name: "@reviewer · codex".to_owned(),
            },
        }],
        position: None,
    };

    assert!(mutation.requires_protocol_two());
    assert!(mutation.requires_protocol_six());
    assert_eq!(mutation.minimum_protocol(), 6);
}

#[test]
fn preservation_of_semantic_inline_metadata_requires_protocol_seven() {
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let mutation = ControlMutation::PreserveAdd {
        operation_id: ids.operation_id(),
        thought_id: ids.thought_id(),
        content: "Press Enter".to_owned(),
        annotations: vec![ContentAnnotation::shortcut(6, 11)],
        position: None,
    };

    assert!(mutation.requires_protocol_seven());
    assert_eq!(mutation.minimum_protocol(), 7);
}

#[test]
fn update_prepare_request_and_receipt_round_trip_over_json() {
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let request = ControlRequest {
        protocol: CONTROL_PROTOCOL_VERSION,
        request_id: ids.request_id(),
        session_id: ids.session_id(),
        mutation: ControlMutation::UpdatePrepare {
            request: UpdatePrepareRequest {
                operation_id: ids.request_id(),
                target_version: StableVersion::parse("1.2.3").expect("version"),
                installation_identity: InstallationIdentity::from_digest([7; 32]),
                deadline: Timestamp::from_millis(9),
            },
        },
    };
    let encoded = serde_json::to_vec(&request).expect("serialize request");
    assert_eq!(
        serde_json::from_slice::<ControlRequest>(&encoded).expect("deserialize request"),
        request
    );
    let response = ControlResponse {
        protocol: request.protocol,
        request_id: request.request_id,
        result: ControlResult::Update(ControlUpdateReceipt::Prepared(UpdatePrepareReply::Ready {
            instance_id: ids.instance_id(),
            session_id: request.session_id,
        })),
    };
    let encoded = serde_json::to_vec(&response).expect("serialize response");
    assert_eq!(
        serde_json::from_slice::<ControlResponse>(&encoded).expect("deserialize response"),
        response
    );
}

#[test]
fn verified_capture_takeover_round_trips_only_on_protocol_five() {
    let mut ids = FakeIdGenerator::new(1_725_201_000_000);
    let owner_instance_id = ids.instance_id();
    let request = ControlRequest {
        protocol: CONTROL_PROTOCOL_VERSION,
        request_id: ids.request_id(),
        session_id: ids.session_id(),
        mutation: ControlMutation::CaptureTakeover {
            expected_owner_instance_id: owner_instance_id,
            requester_instance_id: ids.instance_id(),
            capture_protocol: CAPTURE_CONTROL_PROTOCOL_VERSION,
        },
    };
    assert_eq!(request.mutation.minimum_protocol(), 5);
    let encoded = serde_json::to_vec(&request).expect("serialize takeover request");
    assert_eq!(
        serde_json::from_slice::<ControlRequest>(&encoded).expect("deserialize request"),
        request
    );
    let response = ControlResponse {
        protocol: request.protocol,
        request_id: request.request_id,
        result: ControlResult::Capture(ControlCaptureReceipt::TakeoverScheduled {
            owner_instance_id,
        }),
    };
    let encoded = serde_json::to_vec(&response).expect("serialize takeover response");
    assert_eq!(
        serde_json::from_slice::<ControlResponse>(&encoded).expect("deserialize response"),
        response
    );
}
