//! Current-contract fixtures for the typed local owner-control protocol.

use proqi::ports::control::{
    CONTROL_PROTOCOL_VERSION, ControlRequest, ControlResponse, ControlResult,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

const REQUEST: &str = include_str!("fixtures/control/v9/add.request.json");
const ACCEPTED: &str = include_str!("fixtures/control/v9/add.accepted.json");
const REJECTED: &str = include_str!("fixtures/control/v9/add.rejected.json");
const PRESERVE: &str = include_str!("fixtures/control/v9/preserve_add.request.json");
const UPDATE_PREPARE: &str = include_str!("fixtures/control/v9/update_prepare.request.json");
const UPDATE_READY: &str = include_str!("fixtures/control/v9/update_prepare.ready.json");
const CAPTURE_TAKEOVER: &str = include_str!("fixtures/control/v9/capture_takeover.request.json");
const CAPTURE_SCHEDULED: &str = include_str!("fixtures/control/v9/capture_takeover.scheduled.json");
const RENAME: &str = include_str!("fixtures/control/v9/rename.request.json");

#[test]
fn current_request_success_and_error_fixtures_round_trip_canonically() {
    let request: ControlRequest = assert_round_trip(REQUEST);
    let accepted: ControlResponse = assert_round_trip(ACCEPTED);
    let rejected: ControlResponse = assert_round_trip(REJECTED);

    assert_eq!(request.protocol, CONTROL_PROTOCOL_VERSION);
    assert_eq!(accepted.protocol, CONTROL_PROTOCOL_VERSION);
    assert_eq!(rejected.protocol, CONTROL_PROTOCOL_VERSION);
    assert!(matches!(accepted.result, ControlResult::Accepted(_)));
    assert!(matches!(rejected.result, ControlResult::Rejected { .. }));
}

#[test]
fn current_preservation_fixture_round_trips_with_closed_semantics() {
    let request: ControlRequest = assert_round_trip(PRESERVE);
    assert_eq!(request.protocol, CONTROL_PROTOCOL_VERSION);
    assert!(matches!(
        request.mutation,
        proqi::ports::control::ControlMutation::PreserveAdd { .. }
    ));
}

#[test]
fn current_update_readiness_fixtures_round_trip_canonically() {
    let request: ControlRequest = assert_round_trip(UPDATE_PREPARE);
    let response: ControlResponse = assert_round_trip(UPDATE_READY);

    assert_eq!(request.protocol, CONTROL_PROTOCOL_VERSION);
    assert_eq!(response.protocol, CONTROL_PROTOCOL_VERSION);
    assert!(matches!(response.result, ControlResult::Update(_)));
}

#[test]
fn current_capture_takeover_fixtures_round_trip_canonically() {
    let request: ControlRequest = assert_round_trip(CAPTURE_TAKEOVER);
    let response: ControlResponse = assert_round_trip(CAPTURE_SCHEDULED);

    assert_eq!(request.protocol, CONTROL_PROTOCOL_VERSION);
    assert_eq!(response.protocol, CONTROL_PROTOCOL_VERSION);
    assert!(matches!(
        request.mutation,
        proqi::ports::control::ControlMutation::CaptureTakeover { .. }
    ));
    assert!(matches!(response.result, ControlResult::Capture(_)));
}

#[test]
fn current_rename_fixture_carries_its_durable_browser_identity() {
    let request: ControlRequest = assert_round_trip(RENAME);
    assert!(matches!(
        request.mutation,
        proqi::ports::control::ControlMutation::RenameSession { .. }
    ));
    assert!(request.mutation.durable_operation_id().is_some());
}

#[test]
fn wire_deserialization_rejects_a_request_identity_with_the_wrong_prefix() {
    let mut value: Value = serde_json::from_str(REQUEST).expect("request fixture");
    value["request_id"] = Value::String("op_06g30t8fudrq55fdkjqr6mpe44".to_owned());
    let error = serde_json::from_value::<ControlRequest>(value).expect_err("wrong request prefix");
    assert!(
        error
            .to_string()
            .contains("expected identifier prefix req_")
    );
}

fn assert_round_trip<T>(fixture: &str) -> T
where
    T: DeserializeOwned + Serialize,
{
    let expected: Value = serde_json::from_str(fixture).expect("JSON fixture");
    let typed: T = serde_json::from_value(expected.clone()).expect("typed fixture");
    assert_eq!(
        serde_json::to_value(&typed).expect("serialize fixture"),
        expected
    );
    typed
}
