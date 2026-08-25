use serde_json::json;

use super::{MAX_THOUGHT_STDIN_BYTES, Outcome};

pub(super) fn outcome() -> Outcome {
    Outcome {
        data: json!({
            "cli_schema_version": 1,
            "identifier_encoding": "prefix_base32hex_uuidv7",
            "commands": ["sessions", "thoughts", "update"],
            "explicit_update_check": true,
            "active_session_control": true,
            "control_protocol": crate::ports::control::CONTROL_PROTOCOL_VERSION,
            "cross_session_transfer": true,
            "max_thought_stdin_bytes": MAX_THOUGHT_STDIN_BYTES,
            "herdr_submission": true,
            "herdr_managed_pane_required": true,
        }),
        human: "CLI schema 1\nSessions and thoughts are available\nActive control: available\nHerdr submission: supported in a managed Herdr pane".to_owned(),
    }
}
