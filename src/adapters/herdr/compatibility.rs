//! Typed compatibility policy for the Herdr contracts consumed by Proqi.

use serde_json::{Map, Value};

use crate::ports::agent::AgentError;

use super::contract::{SchemaDocument, Snapshot};

const SUPPORTED_SCHEMA: u32 = 1;
const FIRST_SUPPORTED_PROTOCOL: u32 = 19;
const LAST_SUPPORTED_PROTOCOL: u32 = 20;
const POLICY_DESCRIPTION: &str = "requires Herdr schema 1, protocol 19 or 20, and the compatible agent.prompt request and agent_prompted response contract";

const PROMPT_PARAMS_REF: &str = "#/schemas/request/$defs/AgentPromptParams";
const PROMPT_WAIT_REF: &str = "#/schemas/request/$defs/AgentPromptWaitOptions";
const REQUEST_AGENT_STATUS_REF: &str = "#/schemas/request/$defs/AgentStatus";
const RESPONSE_RESULT_REF: &str = "#/schemas/success_response/$defs/ResponseResult";
const AGENT_INFO_REF: &str = "#/schemas/success_response/$defs/AgentInfo";
const AGENT_SESSION_REF: &str = "#/schemas/success_response/$defs/AgentSessionInfo";
const AGENT_STATUS_REF: &str = "#/schemas/success_response/$defs/AgentStatus";
const SESSION_KIND_REF: &str = "#/schemas/success_response/$defs/AgentSessionRefKind";

/// A protocol accepted by the complete schema and live-snapshot policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CompatibleProtocol(u32);

impl CompatibleProtocol {
    pub(super) const fn value(self) -> u32 {
        self.0
    }
}

/// The single compatibility owner for every semantic Herdr consumer.
pub(super) struct HerdrCompatibilityPolicy;

impl HerdrCompatibilityPolicy {
    pub(super) fn negotiate(
        schema: &SchemaDocument,
        live: &Snapshot,
    ) -> Result<CompatibleProtocol, AgentError> {
        let result = Self::validate_versions(schema, live)
            .and_then(|()| validate_prompt_request(&schema.schemas))
            .and_then(|()| validate_prompt_response(&schema.schemas));
        result.map_or_else(
            |reason| {
                Err(AgentError::Unsupported(format!(
                    "{POLICY_DESCRIPTION}; received schema {} and protocols {}/{} ({reason})",
                    schema.schema_version, schema.protocol, live.protocol
                )))
            },
            |()| Ok(CompatibleProtocol(schema.protocol)),
        )
    }

    fn validate_versions(schema: &SchemaDocument, live: &Snapshot) -> Result<(), &'static str> {
        if schema.schema_version != SUPPORTED_SCHEMA {
            return Err("unsupported schema version");
        }
        if !(FIRST_SUPPORTED_PROTOCOL..=LAST_SUPPORTED_PROTOCOL).contains(&schema.protocol) {
            return Err("unsupported protocol version");
        }
        if live.protocol != schema.protocol {
            return Err("schema and live snapshot protocols disagree");
        }
        if live.version.trim().is_empty() {
            return Err("live snapshot version is missing");
        }
        Ok(())
    }
}

fn validate_prompt_request(schemas: &Value) -> Result<(), &'static str> {
    let operation =
        unique_const_variant(schemas.pointer("/request/oneOf"), "method", "agent.prompt")?;
    require_object(operation, &["method", "params"])?;
    let fields = properties(operation)?;
    require_string_const(fields.get("method"), "agent.prompt")?;
    require_ref(fields.get("params"), PROMPT_PARAMS_REF)?;

    let params = required_value(schemas.pointer("/request/$defs/AgentPromptParams"))?;
    require_object(params, &["target", "text"])?;
    let fields = properties(params)?;
    require_string(fields.get("target"))?;
    require_string(fields.get("text"))?;
    require_nullable_ref(fields.get("wait"), PROMPT_WAIT_REF)?;
    validate_prompt_wait(schemas)
}

fn validate_prompt_wait(schemas: &Value) -> Result<(), &'static str> {
    let wait = required_value(schemas.pointer("/request/$defs/AgentPromptWaitOptions"))?;
    require_object_type(wait)?;
    require_no_required_fields(wait)?;
    let fields = properties(wait)?;
    require_nullable_unsigned(fields.get("timeout_ms"))?;
    require_array_ref(fields.get("until"), REQUEST_AGENT_STATUS_REF)?;
    require_string_enum(
        schemas.pointer("/request/$defs/AgentStatus"),
        &["idle", "working", "blocked", "done", "unknown"],
    )
}

fn validate_prompt_response(schemas: &Value) -> Result<(), &'static str> {
    let success = required_value(schemas.pointer("/success_response"))?;
    require_response_object(success, &["id", "result"])?;
    let success_fields = properties(success)?;
    require_string(success_fields.get("id"))?;
    require_ref(success_fields.get("result"), RESPONSE_RESULT_REF)?;

    let response = unique_const_variant(
        schemas.pointer("/success_response/$defs/ResponseResult/oneOf"),
        "type",
        "agent_prompted",
    )?;
    require_response_object(response, &["type", "agent"])?;
    let fields = properties(response)?;
    require_string_const(fields.get("type"), "agent_prompted")?;
    require_ref(fields.get("agent"), AGENT_INFO_REF)?;
    validate_agent_info(schemas)?;
    validate_agent_session(schemas)
}

fn validate_agent_info(schemas: &Value) -> Result<(), &'static str> {
    let agent = required_value(schemas.pointer("/success_response/$defs/AgentInfo"))?;
    require_response_object(
        agent,
        &[
            "terminal_id",
            "agent_status",
            "workspace_id",
            "tab_id",
            "pane_id",
            "focused",
            "revision",
        ],
    )?;
    let fields = properties(agent)?;
    require_string(fields.get("workspace_id"))?;
    require_string(fields.get("tab_id"))?;
    require_string(fields.get("pane_id"))?;
    require_nullable_string(fields.get("agent"))?;
    require_ref(fields.get("agent_status"), AGENT_STATUS_REF)?;
    require_nullable_ref(fields.get("agent_session"), AGENT_SESSION_REF)?;
    require_string_enum(
        schemas.pointer("/success_response/$defs/AgentStatus"),
        &["idle", "working", "blocked", "done", "unknown"],
    )
}

fn validate_agent_session(schemas: &Value) -> Result<(), &'static str> {
    let session = required_value(schemas.pointer("/success_response/$defs/AgentSessionInfo"))?;
    require_response_object(session, &["source", "agent", "kind", "value"])?;
    let fields = properties(session)?;
    require_string(fields.get("source"))?;
    require_string(fields.get("agent"))?;
    require_ref(fields.get("kind"), SESSION_KIND_REF)?;
    require_string(fields.get("value"))?;
    require_string_enum(
        schemas.pointer("/success_response/$defs/AgentSessionRefKind"),
        &["id", "path"],
    )
}

fn unique_const_variant<'a>(
    variants: Option<&'a Value>,
    field: &str,
    expected: &str,
) -> Result<&'a Value, &'static str> {
    let variants = variants
        .and_then(Value::as_array)
        .ok_or("required schema variant list is missing")?;
    let matches = variants
        .iter()
        .filter(|variant| {
            variant
                .pointer(&format!("/properties/{field}/const"))
                .and_then(Value::as_str)
                == Some(expected)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [value] => Ok(value),
        [] => Err("required schema operation is missing"),
        _ => Err("required schema operation is ambiguous"),
    }
}

fn require_object(value: &Value, required: &[&str]) -> Result<(), &'static str> {
    require_object_type(value)?;
    let actual = required_fields(value)?;
    let matches = actual.len() == required.len()
        && required
            .iter()
            .all(|expected| actual.iter().any(|value| value.as_str() == Some(expected)));
    matches.then_some(()).ok_or("required field list changed")
}

fn require_response_object(value: &Value, required: &[&str]) -> Result<(), &'static str> {
    require_object_type(value)?;
    let actual = required_fields(value)?;
    required
        .iter()
        .all(|expected| actual.iter().any(|value| value.as_str() == Some(expected)))
        .then_some(())
        .ok_or("required field list changed")
}

fn require_object_type(value: &Value) -> Result<(), &'static str> {
    (value.get("type").and_then(Value::as_str) == Some("object"))
        .then_some(())
        .ok_or("required object schema changed")
}

fn required_fields(value: &Value) -> Result<&Vec<Value>, &'static str> {
    value
        .get("required")
        .and_then(Value::as_array)
        .ok_or("required field list is missing")
}

fn require_no_required_fields(value: &Value) -> Result<(), &'static str> {
    value
        .get("required")
        .is_none_or(|required| required.as_array().is_some_and(Vec::is_empty))
        .then_some(())
        .ok_or("optional field list changed")
}

fn properties(value: &Value) -> Result<&Map<String, Value>, &'static str> {
    value
        .get("properties")
        .and_then(Value::as_object)
        .ok_or("required properties are missing")
}

fn required_value(value: Option<&Value>) -> Result<&Value, &'static str> {
    value.ok_or("required schema definition is missing")
}

fn require_string(value: Option<&Value>) -> Result<(), &'static str> {
    (value
        .and_then(|value| value.get("type"))
        .and_then(Value::as_str)
        == Some("string"))
    .then_some(())
    .ok_or("required string field changed")
}

fn require_nullable_string(value: Option<&Value>) -> Result<(), &'static str> {
    let types = value
        .and_then(|value| value.get("type"))
        .and_then(Value::as_array)
        .ok_or("required nullable string field changed")?;
    let expected = ["string", "null"];
    (types.len() == expected.len()
        && expected
            .iter()
            .all(|expected| types.iter().any(|value| value.as_str() == Some(expected))))
    .then_some(())
    .ok_or("required nullable string field changed")
}

fn require_string_const(value: Option<&Value>, expected: &str) -> Result<(), &'static str> {
    require_string(value)?;
    (value
        .and_then(|value| value.get("const"))
        .and_then(Value::as_str)
        == Some(expected))
    .then_some(())
    .ok_or("required operation constant changed")
}

fn require_ref(value: Option<&Value>, expected: &str) -> Result<(), &'static str> {
    (value
        .and_then(|value| value.get("$ref"))
        .and_then(Value::as_str)
        == Some(expected))
    .then_some(())
    .ok_or("required schema reference changed")
}

fn require_nullable_ref(value: Option<&Value>, expected: &str) -> Result<(), &'static str> {
    let variants = value
        .and_then(|value| value.get("anyOf"))
        .and_then(Value::as_array)
        .ok_or("required nullable schema reference changed")?;
    let has_ref = variants
        .iter()
        .any(|value| value.get("$ref").and_then(Value::as_str) == Some(expected));
    let has_null = variants
        .iter()
        .any(|value| value.get("type").and_then(Value::as_str) == Some("null"));
    (variants.len() == 2 && has_ref && has_null)
        .then_some(())
        .ok_or("required nullable schema reference changed")
}

fn require_nullable_unsigned(value: Option<&Value>) -> Result<(), &'static str> {
    let value = value.ok_or("required nullable integer field changed")?;
    let types = value
        .get("type")
        .and_then(Value::as_array)
        .ok_or("required nullable integer field changed")?;
    let has_types = types.len() == 2
        && ["integer", "null"]
            .iter()
            .all(|expected| types.iter().any(|value| value.as_str() == Some(expected)));
    (has_types
        && value.get("format").and_then(Value::as_str) == Some("uint64")
        && value.get("minimum").and_then(Value::as_u64) == Some(0))
    .then_some(())
    .ok_or("required nullable integer field changed")
}

fn require_array_ref(value: Option<&Value>, expected: &str) -> Result<(), &'static str> {
    let value = value.ok_or("required array field changed")?;
    (value.get("type").and_then(Value::as_str) == Some("array"))
        .then_some(())
        .ok_or("required array field changed")?;
    require_ref(value.get("items"), expected)
}

fn require_string_enum(value: Option<&Value>, expected: &[&str]) -> Result<(), &'static str> {
    let value = required_value(value)?;
    require_string(Some(value))?;
    let actual = value
        .get("enum")
        .and_then(Value::as_array)
        .ok_or("required enum changed")?;
    (actual.len() == expected.len()
        && expected
            .iter()
            .all(|expected| actual.iter().any(|value| value.as_str() == Some(expected))))
    .then_some(())
    .ok_or("required enum changed")
}
