//! Installed CLI contract discovery for scripts and the Proqi skill.

use clap::CommandFactory as _;
use serde_json::{Map, Value, json};

use super::{Outcome, helpers::MAX_THOUGHT_STDIN_BYTES};
use crate::{
    adapters::herdr::TOGGLE_CAPABILITY,
    cli::{args::Cli, error_code::ErrorCode},
};

/// Families whose operations and options form the scriptable mutation surface.
const OPTION_FAMILIES: &[&str] = &["sessions", "items", "thoughts"];
/// Process-wide options that every operation inherits.
const GLOBAL_OPTIONS: &[&str] = &["json", "state-dir", "help"];

pub(super) fn outcome() -> Outcome {
    let active_control = cfg!(unix);
    let active_control_summary = if active_control {
        "available"
    } else {
        "unavailable on this platform"
    };
    let mut data = json!({
        "cli_schema_version": 1,
        "identifier_encoding": "prefix_base32hex_uuidv7",
        "commands": [
            "capabilities", "completions", "update", "diagnostics", "doctor",
            "sessions", "items", "thoughts", "herdr"
        ],
        "operations": {
            "diagnostics": ["collect", "keypress"],
            "sessions": [
                "list", "ensure", "create", "rename", "trash", "restore", "undo", "redo",
                "prune"
            ],
            "items": ["insert-separator", "move", "delete", "duplicate"],
            "thoughts": [
                "list", "inspect", "add", "delete", "rename", "replace", "collapse",
                "move", "split", "extract", "merge", "reflow", "export", "send", "undo", "redo"
            ],
            "update": ["check"],
            "herdr": ["toggle"],
            "history_scopes": ["board", "editor", "browser"]
        },
        "options": option_inventory(),
        "error_codes": error_codes(),
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
        "atomic_named_sessions": true,
        "named_thought_creation": true,
        "session_operation_identity": true,
        "idempotent_session_trash": true,
        "bounded_lists": true,
        "json_help_and_version": true,
        "plain_text_thought_export": true,
        "max_thought_stdin_bytes": MAX_THOUGHT_STDIN_BYTES,
        "herdr_submission": true,
        "herdr_managed_pane_required": true,
    });
    data[TOGGLE_CAPABILITY] = json!(true);
    Outcome {
        data,
        human: format!(
            "CLI schema 1\nSessions, board items, and thoughts are available\nActive control: {active_control_summary}\nHerdr submission: supported in a managed Herdr pane"
        ),
    }
}

/// Long options accepted by each scripted operation, derived from the parser.
fn option_inventory() -> Value {
    let command = Cli::command();
    let mut families = Map::new();
    for family in OPTION_FAMILIES {
        let Some(subcommand) = command.find_subcommand(family) else {
            continue;
        };
        let operations = subcommand
            .get_subcommands()
            .filter(|operation| !operation.is_hide_set())
            .map(|operation| {
                let options = operation
                    .get_arguments()
                    .filter(|argument| !argument.is_hide_set())
                    .filter_map(|argument| argument.get_long())
                    .filter(|long| !GLOBAL_OPTIONS.contains(long))
                    .collect::<Vec<_>>();
                (operation.get_name().to_owned(), json!(options))
            })
            .collect::<Map<_, _>>();
        families.insert((*family).to_owned(), Value::Object(operations));
    }
    Value::Object(families)
}

/// Stable error codes with their exit status and retry guidance.
fn error_codes() -> Value {
    ErrorCode::ALL
        .iter()
        .map(|code| {
            json!({
                "code": code.as_str(),
                "exit": code.exit(),
                "retry": code.retry().as_str(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::outcome;

    #[test]
    fn option_inventory_advertises_new_options_from_the_parser() {
        let data = outcome().data;
        let options = &data["options"];
        assert_eq!(
            options["sessions"]["ensure"],
            serde_json::json!(["name", "cwd"])
        );
        assert_eq!(
            options["sessions"]["create"],
            serde_json::json!(["name", "cwd", "operation-id"])
        );
        for operation in ["rename", "trash", "restore", "undo", "redo", "prune"] {
            let advertised = options["sessions"][operation]
                .as_array()
                .expect("session operation options");
            assert!(
                advertised.contains(&serde_json::json!("operation-id")),
                "{operation}"
            );
        }
        assert_eq!(
            options["thoughts"]["add"],
            serde_json::json!(["name", "position", "operation-id"])
        );
        for family in ["sessions", "thoughts"] {
            assert_eq!(
                options[family]["list"]
                    .as_array()
                    .expect("list options")
                    .iter()
                    .filter(|option| *option == "limit" || *option == "after")
                    .count(),
                2
            );
        }
        let operations = data["operations"]["sessions"]
            .as_array()
            .expect("session operations");
        assert_eq!(
            operations.len(),
            options["sessions"]
                .as_object()
                .expect("session options")
                .len()
        );
    }
}
