//! Current-contract fixtures for the typed local owner-control protocol.

use proqi::ports::control::{
    CONTROL_PROTOCOL_VERSION, ControlRequest, ControlResponse, ControlResult,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

const REQUEST: &str = include_str!("fixtures/control/v3/add.request.json");
const ACCEPTED: &str = include_str!("fixtures/control/v3/add.accepted.json");
const REJECTED: &str = include_str!("fixtures/control/v3/add.rejected.json");
const UPDATE_PREPARE: &str = include_str!("fixtures/control/v3/update_prepare.request.json");
const UPDATE_READY: &str = include_str!("fixtures/control/v3/update_prepare.ready.json");

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
fn current_update_readiness_fixtures_round_trip_canonically() {
    let request: ControlRequest = assert_round_trip(UPDATE_PREPARE);
    let response: ControlResponse = assert_round_trip(UPDATE_READY);

    assert_eq!(request.protocol, CONTROL_PROTOCOL_VERSION);
    assert_eq!(response.protocol, CONTROL_PROTOCOL_VERSION);
    assert!(matches!(response.result, ControlResult::Update(_)));
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
