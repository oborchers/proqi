//! Cross-process control flood with concurrent terminal input and resize.

use proqi::{adapters::runtime::SystemIdGenerator, ports::environment::IdGenerator as _};

use super::support::{
    expect_command, json_command, raw_input_command, wait_for_control_owner, wait_for_path,
};

#[derive(Clone, Copy)]
enum ExitScenario {
    Quit,
    Terminate,
}

#[derive(Clone, Copy)]
struct OwnerFixture<'a> {
    binary: &'a str,
    state: &'a std::path::Path,
    session: &'a str,
    ready: &'a std::path::Path,
    start: &'a std::path::Path,
    done: &'a std::path::Path,
    transcript: &'a std::path::Path,
}

impl ExitScenario {
    const fn environment_value(self) -> &'static str {
        match self {
            Self::Quit => "quit",
            Self::Terminate => "terminate",
        }
    }
}

#[test]
fn flooded_owner_control_cannot_starve_local_typing_resize_or_quit() {
    run_fairness(ExitScenario::Quit);
}

#[test]
fn termination_during_control_flood_is_bounded_and_durable() {
    run_fairness(ExitScenario::Terminate);
}

fn run_fairness(exit: ExitScenario) {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");
    let ready = state.path().join("fairness-ready");
    let start = state.path().join("fairness-start");
    let done = state.path().join("fairness-done");
    let transcript = state.path().join("fairness-transcript.log");
    let owner = spawn_owner(
        OwnerFixture {
            binary,
            state: state.path(),
            session,
            ready: &ready,
            start: &start,
            done: &done,
            transcript: &transcript,
        },
        exit,
    );
    wait_for_path(&ready);
    wait_for_control_owner(state.path(), session);
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
    let mut client_debug = Vec::new();
    let accepted = accepted_bodies(outcomes, &mut client_debug);
    assert!(
        !accepted.is_empty(),
        "control flood accepted no requests:\n{}",
        client_debug.join("\n\n")
    );

    std::fs::write(&done, b"done").expect("finish owner input");
    assert_owner_success(owner, &transcript, state.path(), &client_debug);

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

fn accepted_bodies(
    outcomes: Vec<(usize, String, std::process::Output)>,
    client_debug: &mut Vec<String>,
) -> Vec<String> {
    let mut accepted = Vec::new();
    for (index, body, output) in outcomes {
        client_debug.push(format!(
            "client {index}: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
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
    accepted
}

fn assert_owner_success(
    owner: std::process::Child,
    transcript: &std::path::Path,
    state: &std::path::Path,
    client_debug: &[String],
) {
    let output = owner.wait_with_output().expect("wait for fairness owner");
    assert!(
        output.status.success(),
        "fairness owner exited with {}\nexpect stderr:\n{}\ntranscript:\n{}\nclients:\n{}\ndiagnostics:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
        read_text(transcript),
        client_debug.join("\n\n"),
        diagnostic_dump(state)
    );
}

fn spawn_owner(fixture: OwnerFixture<'_>, exit: ExitScenario) -> std::process::Child {
    let script = r#"
        log_user 0
        log_file -a $env(PROQI_TEST_TRANSCRIPT)
        set timeout 15
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        close [open $env(PROQI_TEST_READY) w]
        while {![file exists $env(PROQI_TEST_START)]} {
            expect -timeout 0 {
                -re ".+" { exp_continue }
                timeout {}
                eof { exit 92 }
            }
            after 10
        }
        send -- "local input survives"
        stty rows 6 columns 24
        stty rows 28 columns 100
        send -- "\x1b"
        while {![file exists $env(PROQI_TEST_DONE)]} {
            expect -timeout 0 {
                -re ".+" { exp_continue }
                timeout {}
                eof { exit 93 }
            }
            after 10
        }
        if {$env(PROQI_TEST_EXIT) eq "terminate"} {
            system /bin/kill -TERM [exp_pid]
        } else {
            send -- $env(PROQI_TEST_PRIMARY_Q)
        }
        expect -exact "\x1b\[0 q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", fixture.binary)
        .env("PROQI_TEST_STATE", fixture.state)
        .env("PROQI_TEST_SESSION", fixture.session)
        .env("PROQI_TEST_READY", fixture.ready)
        .env("PROQI_TEST_START", fixture.start)
        .env("PROQI_TEST_DONE", fixture.done)
        .env("PROQI_TEST_TRANSCRIPT", fixture.transcript)
        .env("PROQI_TEST_EXIT", exit.environment_value())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn fairness owner")
}

fn read_text(path: &std::path::Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| format!("read {}: {error}", path.display()))
}

fn diagnostic_dump(state: &std::path::Path) -> String {
    let directory = state.join("data/diagnostics");
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return format!("no diagnostics directory at {}", directory.display());
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonl")
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .iter()
        .map(|path| format!("{}:\n{}", path.display(), read_text(path)))
        .collect::<Vec<_>>()
        .join("\n")
}
