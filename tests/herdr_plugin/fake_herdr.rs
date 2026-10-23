//! Scripted Herdr server shared by the plugin action process tests.
//!
//! It answers the bounded Herdr CLI calls the actions make from fixture files
//! and records every call. It owns no scenario or assertion about the actions.

use std::{fs, process::Output};

use serde_json::{Value, json};

use super::support::{Sandbox, stderr, write_tool};

/// A scripted Herdr server for one tab plus an isolated Proqi state.
pub struct FakeHerdr {
    pub sandbox: Sandbox,
    pub work: std::path::PathBuf,
}

impl FakeHerdr {
    pub fn new() -> Self {
        let sandbox = Sandbox::new();
        let work = sandbox.path().join("agent work");
        fs::create_dir_all(work.join("sub dir")).expect("work");
        // The focused split is a subdirectory of this repository; the tab's
        // session must be anchored at the repository root.
        fs::create_dir_all(work.join(".git")).expect("repository marker");
        fs::create_dir_all(sandbox.path().join("herdr")).expect("herdr state");
        let dir = sandbox.path().join("herdr");
        write_tool(
            &sandbox.bin(),
            "herdr",
            &format!(
                r#"dir='{dir}'
printf 'herdr %s\n' "$*" >> '{log}'
case "$1 $2" in
  "pane list") cat "$dir/panes.json" ;;
  "agent list")
    if [ -f "$dir/agents-fail" ]; then
      printf '%s\n' '{{"error":{{"code":"server_busy","message":"busy"}}}}' >&2; exit 1
    elif [ -f "$dir/agents.json" ]; then cat "$dir/agents.json"; else
      printf '%s\n' '{{"id":"x","result":{{"type":"agent_list","agents":[]}}}}'; fi ;;
  "pane process-info")
    if [ -f "$dir/process-$4.json" ]; then cat "$dir/process-$4.json"; else
      printf '%s\n' '{{"error":{{"code":"pane_not_found","message":"pane not found"}}}}' >&2; exit 1; fi ;;
  "plugin pane")
    if [ "$3" = open ]; then cat "$dir/opened.json"; else printf '%s\n' '{{"id":"x","result":{{}}}}'; fi ;;
  *) printf '%s\n' '{{"id":"x","result":{{}}}}' ;;
esac
"#,
                dir = dir.display(),
                log = sandbox.log().display()
            ),
        );
        // macOS may scan a freshly written executable on its first exec. Absorb
        // that one-time delay here, outside the adapter's bounded Herdr calls.
        let warm = std::process::Command::new(sandbox.bin().join("herdr"))
            .arg("warm-up")
            .output()
            .expect("warm fake Herdr");
        assert!(warm.status.success());
        fs::remove_file(sandbox.log()).expect("reset call log");
        Self { sandbox, work }
    }

    pub fn write(&self, name: &str, value: &Value) {
        fs::write(
            self.sandbox.path().join("herdr").join(name),
            value.to_string(),
        )
        .expect("fixture");
    }

    pub fn panes(&self, panes: &[Value]) {
        self.write(
            "panes.json",
            &json!({ "id": "x", "result": { "type": "pane_list", "panes": panes } }),
        );
    }

    pub fn agents(&self, agents: &[Value]) {
        self.write(
            "agents.json",
            &json!({ "id": "x", "result": { "type": "agent_list", "agents": agents } }),
        );
    }

    pub fn opens(&self, pane: &str) {
        self.write(
            "opened.json",
            &json!({ "id": "x", "result": { "type": "plugin_pane_opened",
                "plugin_pane": { "entrypoint": "board", "plugin_id": "proqi",
                    "pane": { "pane_id": pane, "tab_id": "w1:t1" } } } }),
        );
    }

    pub fn process(&self, pane: &str, argv: &[&str], separate_group: bool) {
        let group = if separate_group { 42 } else { 7 };
        self.write(
            &format!("process-{pane}.json"),
            &json!({ "id": "x", "result": { "process_info": { "shell_pid": 7,
                "foreground_process_group_id": group,
                "foreground_processes": [{ "pid": group, "argv": argv }] } } }),
        );
    }

    pub fn toggle(&self, focused: &str, plugin: bool) -> Output {
        self.invoke("toggle", focused, plugin, |_, _| {})
    }

    /// Run `proqi herdr <action>` as Herdr runs a plugin action, letting the
    /// caller extend the invocation context and the process environment.
    pub fn invoke(
        &self,
        action: &str,
        focused: &str,
        plugin: bool,
        customize: impl FnOnce(&mut Value, &mut std::process::Command),
    ) -> Output {
        let mut context = json!({
            "workspace_id": "w1", "workspace_label": "demo", "tab_id": "w1:t1", "tab_label": "1",
            "focused_pane_id": focused, "focused_pane_cwd": self.work.join("sub dir"),
            "workspace_cwd": self.work,
            "invocation_source": "keybinding"
        });
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_proqi"));
        command
            .arg("--json")
            .arg("--state-dir")
            .arg(self.state_dir())
            .args(["herdr", action])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.sandbox.home());
        customize(&mut context, &mut command);
        if plugin {
            command
                .env("HERDR_ENV", "1")
                .env("HERDR_BIN_PATH", self.sandbox.bin().join("herdr"))
                .env("HERDR_PLUGIN_ID", "proqi")
                .env(
                    "HERDR_PLUGIN_STATE_DIR",
                    self.sandbox.path().join("plugin-state"),
                )
                .env("HERDR_PLUGIN_CONTEXT_JSON", context.to_string());
        }
        command.output().expect("run plugin action")
    }

    /// Proqi state shared by every invocation and direct CLI checks.
    pub fn state_dir(&self) -> std::path::PathBuf {
        self.sandbox.path().join("proqi-state")
    }

    pub fn opened_calls(&self) -> usize {
        self.sandbox
            .calls()
            .iter()
            .filter(|call| call.starts_with("herdr plugin pane open"))
            .count()
    }
}

/// Decode the `data` of a successful JSON envelope.
pub fn data(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{:?} {} {}",
        output.status,
        stderr(output),
        String::from_utf8_lossy(&output.stdout)
    );
    let envelope: Value = serde_json::from_slice(&output.stdout).expect("JSON envelope");
    envelope["data"].clone()
}
