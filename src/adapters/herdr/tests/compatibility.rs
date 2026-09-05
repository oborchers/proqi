//! Qualified Herdr protocol 19 and 20 and provisional protocol 21 contracts.
//!
//! The schema fixtures are sanitized projections recorded from the installed
//! 0.8.0 binary and the checksum-verified official 0.8.2 release binary. They
//! retain every schema node consumed by the adapter and no user state. Protocol
//! 21 is a synthetic projection of the protocol 20 schema, not a recording.

use serde_json::{Value, json};

use crate::ports::{
    agent::{AgentError, AgentGateway as _, AgentState, SubmissionRequest},
    environment::{IdGenerator as _, ProcessError},
};

use super::{
    FakeResponse, agent, capability_responses_for_protocol, current,
    discovery_responses_for_protocol, gateway, layout, right_rect, schema, snapshot, source,
    success,
};

#[test]
fn qualified_and_provisional_protocols_pass_the_same_complete_adapter_contract() {
    for protocol in [19, 20, 21] {
        assert_complete_contract(protocol);
    }
}

fn assert_complete_contract(protocol: u32) {
    let context = source();
    let (mut capability_gateway, _) =
        gateway(capability_responses_for_protocol(&context, protocol));
    let capabilities = capability_gateway.capabilities().expect("capabilities");
    assert_eq!(capabilities.protocol, protocol);

    let agents = json!([agent("w1:p2", "w1", "w1:t1", "idle")]);
    let discovery = discovery_responses_for_protocol(
        &context,
        protocol,
        agents.clone(),
        Some(("w1:p2", right_rect())),
    );
    let (mut discovery_gateway, _) = gateway(discovery);
    let targets = discovery_gateway
        .adjacent_targets(&context)
        .expect("adjacent targets");
    let [target] = targets.as_slice() else {
        panic!("one target for protocol {protocol}");
    };
    assert_eq!(target.protocol, protocol);

    let (mut references_gateway, _) = gateway(vec![
        success(schema(protocol)),
        success(reference_snapshot(protocol)),
    ]);
    let references =
        super::super::discovery::live_references(&mut references_gateway).expect("live references");
    assert_eq!(references.references.len(), 1);
    assert_eq!(references.references[0].pane_id(), "w1:p2");

    let mut submission =
        discovery_responses_for_protocol(&context, protocol, agents, Some(("w1:p2", right_rect())));
    submission.push(success(json!({
        "result": {
            "type": "agent_prompted",
            "agent": agent("w1:p2", "w1", "w1:t1", "working"),
            "future_receipt_field": true
        },
        "future_envelope_field": true
    })));
    let (mut submission_gateway, _) = gateway(submission);
    let mut ids = crate::adapters::memory::FakeIdGenerator::new(1_725_200_000_000);
    let receipt = submission_gateway
        .submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: target.clone(),
            content: "protocol compatibility fixture".to_owned(),
        })
        .expect("semantic prompt delivery");
    assert_eq!(receipt.target.protocol, protocol);
    assert_eq!(receipt.post_state, Some(AgentState::Working));
}

fn reference_snapshot(protocol: u32) -> Value {
    json!({
        "result": {"snapshot": {
            "protocol": protocol,
            "version": super::fixture_version(protocol),
            "workspaces": [{"workspace_id":"w1","label":"Fixture workspace"}],
            "tabs": [{"workspace_id":"w1","tab_id":"w1:t1","label":"Fixture tab"}],
            "agents": [{
                "pane_id":"w1:p2", "workspace_id":"w1", "tab_id":"w1:t1",
                "agent":"codex", "name":"fixture", "agent_status":"idle",
                "future_agent_field": {"ignored": true}
            }],
            "future_snapshot_field": [1, 2, 3]
        }},
        "future_envelope_field": true
    })
}

#[test]
fn protocol_schema_and_live_boundaries_fail_closed_with_precise_reasons() {
    for (schema_protocol, live_protocol, reason) in [
        (18, 18, "unsupported protocol version"),
        (22, 22, "unsupported protocol version"),
        (19, 20, "schema and live snapshot protocols disagree"),
    ] {
        assert_unsupported(schema(schema_protocol), snapshot(live_protocol), reason);
    }

    let mut unsupported_schema = schema(19);
    unsupported_schema["schema_version"] = json!(2);
    assert_unsupported(
        unsupported_schema,
        snapshot(19),
        "unsupported schema version",
    );

    let mut missing_version = snapshot(20);
    missing_version["result"]["snapshot"]["version"] = json!("  ");
    assert_unsupported(
        schema(20),
        missing_version,
        "live snapshot version is missing",
    );
}

#[test]
fn changed_required_prompt_schema_entries_fail_closed() {
    for mutation in [
        SchemaMutation::RequestConstant,
        SchemaMutation::RequestRootConstraint,
        SchemaMutation::SharedRequestConstraint,
        SchemaMutation::OverlappingRequestVariant,
        SchemaMutation::RequestRequired,
        SchemaMutation::TargetType,
        SchemaMutation::TextConstraint,
        SchemaMutation::WaitDefinitionMissing,
        SchemaMutation::WaitDefinitionChanged,
        SchemaMutation::WaitConstraint,
        SchemaMutation::ResponseBinding,
        SchemaMutation::ResponseConstant,
        SchemaMutation::ResponseRequired,
        SchemaMutation::AgentIdentityRequired,
        SchemaMutation::SessionShape,
        SchemaMutation::StatusValues,
    ] {
        let mut changed = schema(21);
        mutation.apply(&mut changed);
        assert_unsupported(changed, snapshot(21), mutation.reason());
    }
}

#[derive(Clone, Copy, Debug)]
enum SchemaMutation {
    RequestConstant,
    RequestRootConstraint,
    SharedRequestConstraint,
    OverlappingRequestVariant,
    RequestRequired,
    TargetType,
    TextConstraint,
    WaitDefinitionMissing,
    WaitDefinitionChanged,
    WaitConstraint,
    ResponseBinding,
    ResponseConstant,
    ResponseRequired,
    AgentIdentityRequired,
    SessionShape,
    StatusValues,
}

impl SchemaMutation {
    fn apply(self, schema: &mut Value) {
        match self {
            Self::RequestRootConstraint => {
                schema["schemas"]["request"]["maxProperties"] = json!(0);
                return;
            }
            Self::OverlappingRequestVariant => {
                schema["schemas"]["request"]["oneOf"]
                    .as_array_mut()
                    .expect("recorded request variants")
                    .push(json!({"type": "object"}));
                return;
            }
            Self::SharedRequestConstraint => {
                schema["schemas"]["request"]["properties"]["method"] =
                    json!({"const": "agent.list", "type": "string"});
                return;
            }
            _ => {}
        }
        let (pointer, value) = self.replacement();
        *schema
            .pointer_mut(pointer)
            .expect("recorded schema pointer") = value;
    }

    fn replacement(self) -> (&'static str, Value) {
        let (pointer, value) = match self {
            Self::RequestConstant => (
                "/schemas/request/oneOf/0/properties/method/const",
                json!("agent.send_text"),
            ),
            Self::RequestRootConstraint
            | Self::SharedRequestConstraint
            | Self::OverlappingRequestVariant => ("/schemas/request/type", json!("object")),
            Self::RequestRequired => ("/schemas/request/oneOf/0/required", json!(["method"])),
            Self::TargetType => (
                "/schemas/request/$defs/AgentPromptParams/properties/target/type",
                json!("integer"),
            ),
            Self::TextConstraint => (
                "/schemas/request/$defs/AgentPromptParams/properties/text",
                json!({"type": "string", "maxLength": 1}),
            ),
            Self::WaitDefinitionMissing => {
                ("/schemas/request/$defs/AgentPromptWaitOptions", Value::Null)
            }
            Self::WaitDefinitionChanged => (
                "/schemas/request/$defs/AgentPromptWaitOptions/properties/until/items/$ref",
                json!("#/schemas/success_response/$defs/AgentStatus"),
            ),
            Self::WaitConstraint => (
                "/schemas/request/$defs/AgentPromptWaitOptions/properties/until",
                json!({
                    "items": {"$ref": "#/schemas/request/$defs/AgentStatus"},
                    "type": "array",
                    "maxItems": 1
                }),
            ),
            Self::ResponseBinding => (
                "/schemas/success_response/properties/result/$ref",
                json!("#/schemas/success_response/$defs/AgentInfo"),
            ),
            Self::ResponseConstant => (
                "/schemas/success_response/$defs/ResponseResult/oneOf/0/properties/type/const",
                json!("agent_prompt_queued"),
            ),
            Self::ResponseRequired => (
                "/schemas/success_response/$defs/ResponseResult/oneOf/0/required",
                json!(["type"]),
            ),
            Self::AgentIdentityRequired => (
                "/schemas/success_response/$defs/AgentInfo/required",
                json!([
                    "terminal_id",
                    "agent_status",
                    "workspace_id",
                    "tab_id",
                    "focused",
                    "revision"
                ]),
            ),
            Self::SessionShape => (
                "/schemas/success_response/$defs/AgentSessionInfo/required",
                json!(["source", "agent", "kind"]),
            ),
            Self::StatusValues => (
                "/schemas/success_response/$defs/AgentStatus/enum",
                json!(["idle", "working", "blocked", "done", "unknown", "paused"]),
            ),
        };
        (pointer, value)
    }

    const fn reason(self) -> &'static str {
        match self {
            Self::RequestConstant | Self::ResponseConstant => {
                "required schema operation is missing"
            }
            Self::RequestRootConstraint | Self::TextConstraint | Self::WaitConstraint => {
                "required request constraint changed"
            }
            Self::SharedRequestConstraint => "shared request constraint changed",
            Self::OverlappingRequestVariant => "request operation variants are not const-disjoint",
            Self::RequestRequired
            | Self::ResponseRequired
            | Self::AgentIdentityRequired
            | Self::SessionShape => "required field list changed",
            Self::TargetType => "required string field changed",
            Self::WaitDefinitionMissing => "required object schema changed",
            Self::WaitDefinitionChanged | Self::ResponseBinding => {
                "required schema reference changed"
            }
            Self::StatusValues => "required enum changed",
        }
    }
}

#[test]
fn malformed_timeout_and_additive_unknown_fields_are_handled_without_drift() {
    let (mut timed_out, _) = gateway(vec![FakeResponse::Error(ProcessError::TimedOut)]);
    assert_eq!(timed_out.capabilities(), Err(AgentError::TimedOut));

    let (mut malformed, _) = gateway(vec![success(json!({"not": "a schema"}))]);
    assert!(matches!(
        malformed.capabilities(),
        Err(AgentError::Malformed(_))
    ));

    let context = source();
    let mut additive = schema(21);
    additive["future_schema_field"] = json!({"retained_by_provider": true});
    additive["schemas"]["request"]["oneOf"]
        .as_array_mut()
        .expect("recorded request variants")
        .push(json!({
            "properties": {
                "method": {"const": "future.operation", "type": "string"}
            },
            "required": ["method"],
            "type": "object"
        }));
    add_required_response_field(
        &mut additive["schemas"]["success_response"],
        "future_envelope_field",
    );
    add_required_response_field(
        &mut additive["schemas"]["success_response"]["$defs"]["ResponseResult"]["oneOf"][0],
        "future_receipt_field",
    );
    add_required_response_field(
        &mut additive["schemas"]["success_response"]["$defs"]["AgentInfo"],
        "future_agent_field",
    );
    let (mut compatible, _) = gateway(vec![
        success(additive),
        success(json!({
            "result": {"snapshot": {
                "protocol": 21,
                "version": "provisional-fixture",
                "future_snapshot_field": true
            }},
            "future_envelope_field": true
        })),
        success(current(&context)),
        success(layout(&context)),
    ]);
    assert_eq!(
        compatible.capabilities().expect("additive fields").protocol,
        21
    );
}

fn add_required_response_field(object: &mut Value, field: &str) {
    object["properties"][field] = json!({"type": "string"});
    object["required"]
        .as_array_mut()
        .expect("recorded response required fields")
        .push(json!(field));
}

fn assert_unsupported(schema: Value, snapshot: Value, reason: &str) {
    let (mut gateway, _) = gateway(vec![success(schema), success(snapshot)]);
    let error = gateway.capabilities().expect_err("incompatible contract");
    let AgentError::Unsupported(message) = error else {
        panic!("expected unsupported error, received {error:?}");
    };
    assert!(
        message.contains("qualified protocols 19 through 20, or provisional protocol 21"),
        "{message}"
    );
    assert!(message.contains(reason), "{message}");
}
