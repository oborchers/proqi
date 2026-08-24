//! Cross-process control flood with concurrent terminal input and resize.

use proqi::{adapters::runtime::SystemIdGenerator, ports::environment::IdGenerator as _};

use super::{expect_command, json_command, raw_input_command, wait_for_path};

#[test]
fn flooded_owner_control_cannot_starve_local_typing_resize_or_quit() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");
    let ready = state.path().join("fairness-ready");
    let start = state.path().join("fairness-start");
    let done = state.path().join("fairness-done");
    let mut owner = spawn_owner(binary, state.path(), session, &ready, &start, &done);
    wait_for_path(&ready);
    std::fs::write(&start, b"start").expect("start owner input");

    let state_path = state.path();
    let outcomes = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for index in 0..20 {
            handles.push(scope.spawn(move || {
                let operation = SystemIdGenerator.operation_id().to_string();
                let body = format!("forwarded control {index}");
                let output = raw_input_command(
                    binary,
                    state_path,
                    &["thoughts", "add", session, "--operation-id", &operation],
                    &body,
                );
                (index, body, output)
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().expect("control client"))
            .collect::<Vec<_>>()
    });
    let mut accepted = Vec::new();
    for (index, body, output) in outcomes {
        if output.status.success() {
            accepted.push(body);
            continue;
        }
        let error: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("structured busy result");
        assert!(
            matches!(
                error["error"]["code"].as_str(),
                Some("session_busy" | "operation_indeterminate")
            ),
            "client {index}: {error}"
        );
    }
    assert!(!accepted.is_empty(), "control flood accepted no requests");

    std::fs::write(&done, b"done").expect("finish owner input");
    let status = owner.wait().expect("wait for fairness owner");
    assert!(status.success(), "fairness owner exited with {status}");

    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    let thoughts = thoughts["data"]["thoughts"].as_array().expect("thoughts");
    assert!(thoughts.len() > accepted.len());
    assert!(thoughts.len() <= 21);
    assert!(
        thoughts
            .iter()
            .any(|thought| thought["content"] == "local input survives")
    );
    for expected in accepted {
        assert!(
            thoughts
                .iter()
                .any(|thought| thought["content"] == expected)
        );
    }
}

fn spawn_owner(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    ready: &std::path::Path,
    start: &std::path::Path,
    done: &std::path::Path,
) -> std::process::Child {
    let script = r#"
        log_user 0
        set timeout 15
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        close [open $env(PROQI_TEST_READY) w]
        while {![file exists $env(PROQI_TEST_START)]} { after 10 }
        send -- "nlocal input survives"
        stty rows 6 columns 24
        stty rows 28 columns 100
        send -- "\x1b"
        while {![file exists $env(PROQI_TEST_DONE)]} { after 10 }
        send -- "\x11"
        expect -exact "\x1b\[0 q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_READY", ready)
        .env("PROQI_TEST_START", start)
        .env("PROQI_TEST_DONE", done)
        .spawn()
        .expect("spawn fairness owner")
}
