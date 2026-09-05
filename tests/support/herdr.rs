//! Process fixture for the structured Herdr schema, discovery, and prompt CLI.

use std::{ffi::OsString, path::Path};

pub struct HerdrFixture {
    _temporary: tempfile::TempDir,
    program: std::path::PathBuf,
}

impl HerdrFixture {
    pub fn new(protocol: u32) -> Self {
        let temporary = tempfile::tempdir().expect("Herdr fixture directory");
        let program = temporary.path().join("herdr");
        let prompt_log = temporary.path().join("prompt.bin");
        std::fs::write(&program, fixture_script(protocol, &prompt_log))
            .expect("Herdr fixture executable");
        make_executable(&program);
        Self {
            _temporary: temporary,
            program,
        }
    }

    pub fn program(&self) -> OsString {
        OsString::from(self.program.as_os_str())
    }

    pub fn prompt_bytes(&self) -> Option<Vec<u8>> {
        std::fs::read(self.program.with_file_name("prompt.bin")).ok()
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .expect("Herdr fixture permissions");
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

fn fixture_script(protocol: u32, prompt_log: &Path) -> String {
    let schema = recorded_schema(protocol);
    let version = fixture_version(protocol);
    let template = r#"#!/bin/sh
if [ "$1 $2 $3" = "api schema --json" ]; then
  printf '%s\n' '__SCHEMA__'
elif [ "$1 $2" = "api snapshot" ]; then
  printf '%s\n' '{"result":{"snapshot":{"protocol":__PROTOCOL__,"version":"__VERSION__","workspaces":[{"workspace_id":"w1","label":"Fixture workspace"}],"tabs":[{"workspace_id":"w1","tab_id":"w1:t1","label":"Fixture tab"}],"agents":[{"pane_id":"w1:p2","workspace_id":"w1","tab_id":"w1:t1","agent":"codex","name":"fixture","agent_status":"idle","cwd":"/private/not-a-label","terminal_title":"private prompt","future_agent_field":true}],"future_snapshot_field":true}},"future_envelope_field":true}'
elif [ "$1 $2 $3" = "pane current --current" ]; then
  printf '%s\n' '{"result":{"pane":{"pane_id":"w1:p1","workspace_id":"w1","tab_id":"w1:t1"}}}'
elif [ "$1 $2" = "pane layout" ]; then
  printf '%s\n' '{"result":{"layout":{"workspace_id":"w1","tab_id":"w1:t1","panes":[{"pane_id":"w1:p1","rect":{"x":0,"y":0,"width":20,"height":20}}]}}}'
elif [ "$1 $2" = "agent list" ]; then
  printf '%s\n' '{"result":{"agents":[{"pane_id":"w1:p2","workspace_id":"w1","tab_id":"w1:t1","agent":"codex","name":"fixture","agent_status":"idle","agent_session":{"agent":"codex","kind":"id","source":"herdr:codex","value":"fixture-session"}}]}}'
elif [ "$1 $2" = "pane neighbor" ]; then
  if [ "$6" = "right" ]; then
    printf '%s\n' '{"result":{"neighbor":{"pane_id":"w1:p1","direction":"right","neighbor_pane_id":"w1:p2","layout":{"workspace_id":"w1","tab_id":"w1:t1","panes":[{"pane_id":"w1:p1","rect":{"x":0,"y":0,"width":20,"height":20}},{"pane_id":"w1:p2","rect":{"x":20,"y":0,"width":20,"height":20}}]}}}}'
  else
    printf '{"result":{"neighbor":{"pane_id":"w1:p1","direction":"%s","layout":{"workspace_id":"w1","tab_id":"w1:t1","panes":[{"pane_id":"w1:p1","rect":{"x":0,"y":0,"width":20,"height":20}}]}}}}\n' "$6"
  fi
elif [ "$1 $2 $3" = "agent prompt w1:p2" ]; then
  printf '%s' "$4" > "__PROMPT_LOG__"
  printf '%s\n' '{"result":{"type":"agent_prompted","agent":{"pane_id":"w1:p2","workspace_id":"w1","tab_id":"w1:t1","agent":"codex","name":"fixture","agent_status":"working","agent_session":{"agent":"codex","kind":"id","source":"herdr:codex","value":"fixture-session"}},"future_receipt_field":true}}'
else
  printf '%s\n' '{"error":{"code":"fixture_unknown","message":"unexpected command"}}' >&2
  exit 1
fi
"#;
    template
        .replace("__SCHEMA__", &schema)
        .replace("__PROTOCOL__", &protocol.to_string())
        .replace("__VERSION__", version)
        .replace("__PROMPT_LOG__", &prompt_log.display().to_string())
}

fn recorded_schema(protocol: u32) -> String {
    let recorded = match protocol {
        19 => include_str!("../../src/adapters/herdr/tests/fixtures/protocol19/schema.json"),
        _ => include_str!("../../src/adapters/herdr/tests/fixtures/protocol20/schema.json"),
    };
    let mut schema: serde_json::Value =
        serde_json::from_str(recorded).expect("recorded Herdr schema");
    schema["protocol"] = serde_json::json!(protocol);
    serde_json::to_string(&schema).expect("compact Herdr schema")
}

const fn fixture_version(protocol: u32) -> &'static str {
    match protocol {
        19 => "0.8.0",
        20 => "0.8.2",
        _ => "provisional-fixture",
    }
}
