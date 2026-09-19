use serde_json::json;

use super::{Outcome, helpers::MAX_THOUGHT_STDIN_BYTES};

pub(super) fn outcome() -> Outcome {
    let active_control = cfg!(unix);
    let active_control_summary = if active_control {
        "available"
    } else {
        "unavailable on this platform"
    };
    Outcome {
        data: json!({
            "cli_schema_version": 1,
            "identifier_encoding": "prefix_base32hex_uuidv7",
            "commands": [
                "capabilities", "completions", "update", "diagnostics", "doctor",
                "sessions", "items", "thoughts"
            ],
            "operations": {
                "diagnostics": ["collect", "keypress"],
                "sessions": ["list", "rename", "trash", "restore", "undo", "redo", "prune"],
                "items": ["insert-separator", "move", "delete", "duplicate"],
                "thoughts": [
                    "list", "inspect", "add", "delete", "rename", "replace", "collapse",
                    "move", "split", "extract", "merge", "reflow", "send", "undo", "redo"
                ],
                "update": ["check"],
                "history_scopes": ["board", "editor", "browser"]
            },
            "explicit_update_check": true,
            "active_session_control": active_control,
            "active_session_read_sync": active_control,
            "control_protocol": crate::ports::control::CONTROL_PROTOCOL_VERSION,
            "cross_session_transfer": true,
            "exact_thought_replacement": true,
            "replacement_sha256_precondition": true,
            "durable_thought_collapse": true,
            "durable_visual_separators": true,
            "semantic_separator_mutations": true,
            "mixed_board_item_mutations": true,
            "exact_text_transformations": true,
            "durable_browser_history": true,
            "max_thought_stdin_bytes": MAX_THOUGHT_STDIN_BYTES,
            "herdr_submission": true,
            "herdr_managed_pane_required": true,
        }),
        human: format!(
            "CLI schema 1\nSessions, board items, and thoughts are available\nActive control: {active_control_summary}\nHerdr submission: supported in a managed Herdr pane"
        ),
    }
}
