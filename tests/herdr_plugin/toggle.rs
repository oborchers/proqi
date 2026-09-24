//! The real `proqi herdr toggle` binary against a scripted Herdr: open, repeated
//! focus, close, and dead-pane replacement after a simulated host restart.

use std::{fs, path::Path, process::Output};

use serde_json::{Value, json};

use super::support::{Sandbox, stderr, write_tool};

struct FakeHerdr {
    sandbox: Sandbox,
    work: std::path::PathBuf,
}

impl FakeHerdr {
    fn new() -> Self {
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

    fn write(&self, name: &str, value: &Value) {
        fs::write(
            self.sandbox.path().join("herdr").join(name),
            value.to_string(),
        )
        .expect("fixture");
    }

    fn panes(&self, panes: &[Value]) {
        self.write(
            "panes.json",
            &json!({ "id": "x", "result": { "type": "pane_list", "panes": panes } }),
        );
    }

    fn opens(&self, pane: &str) {
        self.write(
            "opened.json",
            &json!({ "id": "x", "result": { "type": "plugin_pane_opened",
                "plugin_pane": { "entrypoint": "board", "plugin_id": "proqi",
                    "pane": { "pane_id": pane, "tab_id": "w1:t1" } } } }),
        );
    }

    fn process(&self, pane: &str, argv: &[&str], separate_group: bool) {
        let group = if separate_group { 42 } else { 7 };
        self.write(
            &format!("process-{pane}.json"),
            &json!({ "id": "x", "result": { "process_info": { "shell_pid": 7,
                "foreground_process_group_id": group,
                "foreground_processes": [{ "pid": group, "argv": argv }] } } }),
        );
    }

    fn toggle(&self, focused: &str, plugin: bool) -> Output {
        let context = json!({
            "workspace_id": "w1", "workspace_label": "demo", "tab_id": "w1:t1", "tab_label": "1",
            "focused_pane_id": focused, "focused_pane_cwd": self.work.join("sub dir"),
            "workspace_cwd": self.work,
            "invocation_source": "keybinding"
        });
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_proqi"));
        command
            .arg("--json")
            .arg("--state-dir")
            .arg(self.sandbox.path().join("proqi-state"))
            .args(["herdr", "toggle"])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.sandbox.home());
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
        command.output().expect("run toggle")
    }

    fn opened_calls(&self) -> usize {
        self.sandbox
            .calls()
            .iter()
            .filter(|call| call.starts_with("herdr plugin pane open"))
            .count()
    }
}

fn data(output: &Output) -> Value {
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

fn agent() -> Value {
    json!({ "pane_id": "w1:p1", "tab_id": "w1:t1", "agent": "claude", "cwd": "/work" })
}

fn companion(pane: &str, live: bool) -> Value {
    let mut value = json!({ "pane_id": pane, "tab_id": "w1:t1", "label": "Proqi" });
    if live {
        value["display_agent"] = json!("proqi");
    }
    value
}

#[test]
fn toggle_outside_a_herdr_plugin_action_is_unsupported() {
    let herdr = FakeHerdr::new();
    let output = herdr.toggle("w1:p1", false);
    assert_eq!(output.status.code(), Some(6));
    let envelope: Value = serde_json::from_slice(&output.stdout).expect("JSON error");
    assert_eq!(envelope["error"]["code"], "unsupported");
    assert!(herdr.sandbox.calls().is_empty());
}

#[test]
fn toggle_opens_once_focuses_repeatedly_closes_and_replaces_a_dead_pane() {
    let herdr = FakeHerdr::new();
    herdr.panes(&[agent()]);
    herdr.opens("w1:p2");
    let opened = data(&herdr.toggle("w1:p1", true));
    assert_eq!(opened["action"], "opened");
    assert_eq!(opened["pane_id"], "w1:p2");
    let session = opened["session_id"].as_str().expect("session").to_owned();
    let open = herdr
        .sandbox
        .calls()
        .into_iter()
        .find(|call| call.contains("pane open"))
        .expect("open");
    assert!(
        open.contains(&format!("--env PROQI_HERDR_SESSION={session} --focus")),
        "{open}"
    );
    assert!(
        open.contains("--target-pane w1:p1 --direction right"),
        "{open}"
    );
    assert_named_session(&herdr, &session);

    herdr.panes(&[agent(), companion("w1:p2", true)]);
    herdr.process("w1:p2", &["/opt/bin/proqi", "--resume", &session], false);
    for _ in 0..3 {
        assert_eq!(data(&herdr.toggle("w1:p1", true))["action"], "focused");
    }
    assert_eq!(
        herdr.opened_calls(),
        1,
        "repeated toggles never open a second pane"
    );

    // No Proqi owns the session in this scripted Herdr, so nothing can confirm
    // that pending edits are durable. The toggle must refuse to close.
    let refused = herdr.toggle("w1:p2", true);
    assert_eq!(refused.status.code(), Some(5), "{}", stderr(&refused));
    let envelope: Value = serde_json::from_slice(&refused.stdout).expect("JSON error");
    assert_eq!(envelope["error"]["code"], "session_busy");
    assert!(
        !herdr
            .sandbox
            .calls()
            .iter()
            .any(|call| call.starts_with("herdr pane close"))
    );

    herdr.panes(&[agent()]);
    herdr.opens("w1:p3");
    let reopened = data(&herdr.toggle("w1:p1", true));
    assert_eq!(
        reopened["session_id"],
        session.as_str(),
        "the tab keeps one session"
    );

    // A host cold restart leaves the companion pane as an idle login shell.
    herdr.panes(&[agent(), companion("w1:p3", false)]);
    herdr.process("w1:p3", &["-zsh"], false);
    herdr.opens("w1:p4");
    let replaced = data(&herdr.toggle("w1:p1", true));
    assert_eq!(replaced["action"], "opened");
    assert_eq!(replaced["replaced_pane_id"], "w1:p3");
    assert_eq!(replaced["session_id"], session.as_str());
    let calls = herdr.sandbox.calls();
    let open_p4 = calls
        .iter()
        .rposition(|call| call.contains("pane open"))
        .expect("open");
    let close_p3 = calls
        .iter()
        .position(|call| call == "herdr pane close w1:p3")
        .expect("close");
    assert!(
        open_p4 < close_p3,
        "the replacement opens before the dead pane closes"
    );
    assert_eq!(herdr.opened_calls(), 3);
}

#[test]
fn a_recorded_pane_now_running_other_work_is_left_untouched() {
    let herdr = FakeHerdr::new();
    herdr.panes(&[agent()]);
    herdr.opens("w1:p2");
    data(&herdr.toggle("w1:p1", true));
    herdr.panes(&[agent(), companion("w1:p2", false)]);
    herdr.process("w1:p2", &["vim", "notes.md"], true);
    herdr.opens("w1:p3");
    let opened = data(&herdr.toggle("w1:p1", true));
    assert_eq!(opened["replaced_pane_id"], Value::Null);
    assert!(
        !herdr
            .sandbox
            .calls()
            .iter()
            .any(|call| call.starts_with("herdr pane close"))
    );
}

fn assert_named_session(herdr: &FakeHerdr, session: &str) {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_proqi"))
        .args(["--json", "--state-dir"])
        .arg(herdr.sandbox.path().join("proqi-state"))
        .args(["sessions", "list"])
        .env("HOME", herdr.sandbox.home())
        .output()
        .expect("list sessions");
    let listed: Value = serde_json::from_slice(&output.stdout).expect("sessions");
    let sessions = listed["data"]["sessions"]
        .as_array()
        .expect("sessions array");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["id"], session);
    assert_eq!(sessions[0]["name"], "demo-w1-t1");
    let origin = sessions[0]["origin_cwd"].as_str().expect("origin");
    assert_eq!(
        Path::new(origin),
        fs::canonicalize(&herdr.work).expect("canonical work")
    );
}

#[test]
fn toggling_from_a_proqi_the_plugin_did_not_open_returns_to_the_agent_without_closing_it() {
    let herdr = FakeHerdr::new();
    herdr.panes(&[agent(), companion("w1:p5", true)]);
    herdr.process("w1:p5", &["proqi", "--resume", "manual"], true);
    let returned = data(&herdr.toggle("w1:p5", true));
    assert_eq!(returned["action"], "returned");
    assert_eq!(returned["pane_id"], "w1:p1");
    let calls = herdr.sandbox.calls();
    assert!(
        !calls
            .iter()
            .any(|call| call.starts_with("herdr pane close"))
    );
    assert!(!calls.iter().any(|call| call.contains("pane open")));
}
