//! Move ordering and recoverable persistence failures for active annotated transfers.

use proqi::{adapters::runtime::SystemIdGenerator, ports::environment::IdGenerator as _};

use super::{
    active_transfer::{TransferFixture, assert_destination_database},
    support::{
        expect_command, json_command, raw_input_command, wait_for_control_owner, wait_for_path,
    },
};

#[test]
fn annotated_move_waits_for_destination_durability_and_survives_missing_attachment_check() {
    let fixture = TransferFixture::new("Moved Grüße 第一 image.png");
    std::fs::remove_file(&fixture.image).expect("make attachment inaccessible");
    let (owner, done) = fixture.start_owner("move");
    let operation = SystemIdGenerator.operation_id().to_string();
    let removal = SystemIdGenerator.operation_id().to_string();
    let moved = send_move(&fixture, &operation, &removal);
    assert!(
        moved.status.success(),
        "active annotated move failed: {}",
        String::from_utf8_lossy(&moved.stdout)
    );
    let moved: serde_json::Value = serde_json::from_slice(&moved.stdout).expect("move JSON");
    assert_eq!(moved["data"]["destination_receipt"]["sequence"], 1);
    assert_eq!(moved["data"]["source_removal_receipt"]["sequence"], 2);
    assert_eq!(moved["data"]["source_removed"], true);
    let destination_thought = moved["data"]["destination_thought_id"]
        .as_str()
        .expect("destination thought ID");
    assert_destination_database(
        fixture.state(),
        &fixture.destination,
        destination_thought,
        fixture.image.to_string_lossy().as_ref(),
        (1, 1, 1, 1),
    );
    assert!(
        json_command(
            fixture.binary,
            fixture.state(),
            &["thoughts", "list", &fixture.source]
        )["data"]["items"]
            .as_array()
            .expect("source thoughts")
            .is_empty()
    );

    let replay = send_move(&fixture, &operation, &removal);
    assert!(replay.status.success());
    let replay: serde_json::Value = serde_json::from_slice(&replay.stdout).expect("replay JSON");
    assert_eq!(
        replay["data"]["destination_receipt"]["idempotent_replay"],
        true
    );
    assert_eq!(
        replay["data"]["source_removal_receipt"]["idempotent_replay"],
        true
    );

    wait_for_missing_attachment_diagnostic(fixture.state());
    fixture.finish_owner(owner, &done);
}

#[test]
fn failed_active_transfer_retains_source_and_exact_retry_recovers_without_sequence_gap() {
    let fixture = TransferFixture::new("Retry Grüße 第一 image.png");
    let owner = FailingOwner::start(&fixture);
    let operation = SystemIdGenerator.operation_id().to_string();
    let removal = SystemIdGenerator.operation_id().to_string();
    let failed_transfer = send_move(&fixture, &operation, &removal);
    assert!(!failed_transfer.status.success());
    let failure: serde_json::Value =
        serde_json::from_slice(&failed_transfer.stdout).expect("failure JSON");
    assert_eq!(failure["error"]["code"], "storage_busy");
    owner.assert_failed();
    assert_eq!(
        json_command(
            fixture.binary,
            fixture.state(),
            &["thoughts", "list", &fixture.source]
        )["data"]["items"]
            .as_array()
            .expect("source thoughts")
            .len(),
        1,
        "failed destination persistence must retain the source"
    );

    owner.retry_and_wait();
    let replay = send_move(&fixture, &operation, &removal);
    assert!(
        replay.status.success(),
        "replay after retry failed: {}",
        String::from_utf8_lossy(&replay.stdout)
    );
    let replay: serde_json::Value = serde_json::from_slice(&replay.stdout).expect("replay JSON");
    assert_eq!(replay["data"]["destination_receipt"]["sequence"], 1);
    assert_eq!(
        replay["data"]["destination_receipt"]["idempotent_replay"],
        true
    );
    assert_eq!(replay["data"]["source_removed"], true);
    assert_eq!(replay["data"]["source_removal_receipt"]["sequence"], 2);
    assert!(
        json_command(
            fixture.binary,
            fixture.state(),
            &["thoughts", "list", &fixture.source]
        )["data"]["items"]
            .as_array()
            .expect("source thoughts after retry")
            .is_empty()
    );
    let destination_thought = replay["data"]["destination_thought_id"]
        .as_str()
        .expect("destination thought ID");
    assert_destination_database(
        fixture.state(),
        &fixture.destination,
        destination_thought,
        fixture.image.to_string_lossy().as_ref(),
        (1, 1, 1, 1),
    );

    let next = raw_input_command(
        fixture.binary,
        fixture.state(),
        &["thoughts", "add", &fixture.destination],
        "ordinary write after exact retry",
    );
    assert!(next.status.success());
    let next: serde_json::Value = serde_json::from_slice(&next.stdout).expect("next write JSON");
    assert_eq!(next["data"]["receipt"]["sequence"], 2);
    owner.finish(&fixture);
}

#[test]
fn annotated_transfer_flushes_a_live_destination_draft_before_its_own_commit() {
    let fixture = TransferFixture::new("Draft Grüße 第一 image.png");
    let anchor = super::support::json_input_command(
        fixture.binary,
        fixture.state(),
        &["thoughts", "add", &fixture.destination],
        "anchor",
    );
    let anchor_id = anchor["data"]["thought_id"]
        .as_str()
        .expect("anchor thought ID");
    let barrier = DraftBarrier::new(fixture.state());
    let owner = spawn_editing_owner(
        fixture.binary,
        fixture.state(),
        &fixture.destination,
        &barrier,
    );
    wait_for_path(&barrier.pending);
    wait_for_control_owner(fixture.state(), &fixture.destination);

    let operation = SystemIdGenerator.operation_id().to_string();
    let transferred = send_while_draft_is_pending(&fixture, &operation, &barrier);
    assert!(
        transferred.status.success(),
        "transfer during live draft failed: {}",
        String::from_utf8_lossy(&transferred.stdout)
    );
    let transferred: serde_json::Value =
        serde_json::from_slice(&transferred.stdout).expect("transfer JSON");
    assert_eq!(transferred["data"]["destination_receipt"]["sequence"], 3);
    let transferred_id = transferred["data"]["destination_thought_id"]
        .as_str()
        .expect("destination thought ID");
    let live = json_command(
        fixture.binary,
        fixture.state(),
        &["thoughts", "list", &fixture.destination],
    );
    assert_eq!(live["data"]["items"][0]["id"], anchor_id);
    assert_eq!(live["data"]["items"][0]["content"], "anchor pending draft");
    assert_destination_database(
        fixture.state(),
        &fixture.destination,
        transferred_id,
        fixture.image.to_string_lossy().as_ref(),
        (3, 2, 3, 1),
    );

    fixture.finish_owner(owner, &barrier.done);
}

fn send_while_draft_is_pending(
    fixture: &TransferFixture,
    operation: &str,
    barrier: &DraftBarrier,
) -> std::process::Output {
    let binary = fixture.binary;
    let state = fixture.state().to_path_buf();
    let source = fixture.source.clone();
    let source_thought = fixture.source_thought.clone();
    let destination = fixture.destination.clone();
    let transfer_operation = operation.to_owned();
    let queued = barrier.queued.clone();
    let release = barrier.release.clone();
    std::thread::scope(move |scope| {
        let transfer = scope.spawn(move || {
            raw_input_command(
                binary,
                &state,
                &[
                    "thoughts",
                    "send",
                    &source,
                    &source_thought,
                    &destination,
                    "--operation-id",
                    &transfer_operation,
                ],
                "",
            )
        });
        wait_for_path(&queued);
        std::fs::write(&release, b"release").expect("release pending draft barrier");
        transfer.join().expect("join transfer request")
    })
}

fn spawn_editing_owner(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    barrier: &DraftBarrier,
) -> std::process::Child {
    let script = r#"
        log_user 0
        set timeout 12
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        send "\r"
        set accepted "$env(PROQI_TEST_STATE)/runtime/input-accepted"
        set edit_deadline [expr {[clock milliseconds] + 5000}]
        while {1} {
            if {[file exists $accepted]} {
                set channel [open $accepted r]
                set proof [read $channel]
                close $channel
                if {[string first "mode=edit" $proof] >= 0} { break }
            }
            if {[clock milliseconds] >= $edit_deadline} { exit 90 }
            expect -timeout 0 {
                -re ".+" { exp_continue }
                timeout {}
                eof { exit 89 }
            }
            after 5
        }
        close [open $env(PROQI_TEST_INPUT_BARRIER_ARM) w]
        send -- "\x1b\[200~ pending draft\x1b\[201~"
        set deadline [expr {[clock milliseconds] + 30000}]
        while {![file exists $env(PROQI_TEST_DONE)]} {
            if {[clock milliseconds] >= $deadline} { exit 91 }
            expect -timeout 0 {
                -re ".+" { exp_continue }
                timeout {}
                eof { exit 92 }
            }
            after 20
        }
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    super::support::expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_INPUT_STALL", "1")
        .env("PROQI_TEST_INPUT_ACCEPTANCE", "1")
        .env("PROQI_TEST_INPUT_BARRIER_ARM", &barrier.arm)
        .env("PROQI_TEST_INPUT_BARRIER_PENDING", &barrier.pending)
        .env("PROQI_TEST_INPUT_BARRIER_RELEASE", &barrier.release)
        .env("PROQI_TEST_CONTROL_QUEUED", &barrier.queued)
        .env("PROQI_TEST_DONE", &barrier.done)
        .spawn()
        .expect("spawn editing destination owner")
}

struct DraftBarrier {
    arm: std::path::PathBuf,
    pending: std::path::PathBuf,
    queued: std::path::PathBuf,
    release: std::path::PathBuf,
    done: std::path::PathBuf,
}

impl DraftBarrier {
    fn new(state: &std::path::Path) -> Self {
        Self {
            arm: state.join("draft-barrier-arm"),
            pending: state.join("draft-is-pending"),
            queued: state.join("transfer-is-queued"),
            release: state.join("release-draft-barrier"),
            done: state.join("draft-done"),
        }
    }
}

fn send_move(fixture: &TransferFixture, operation: &str, removal: &str) -> std::process::Output {
    raw_input_command(
        fixture.binary,
        fixture.state(),
        &[
            "thoughts",
            "send",
            &fixture.source,
            &fixture.source_thought,
            &fixture.destination,
            "--remove",
            "--operation-id",
            operation,
            "--remove-operation-id",
            removal,
        ],
        "",
    )
}

fn diagnostic_content(state: &std::path::Path) -> String {
    let directory = state.join("data/diagnostics");
    std::fs::read_dir(directory)
        .expect("diagnostics directory")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|value| value == "jsonl")
        })
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .collect()
}

fn wait_for_missing_attachment_diagnostic(state: &std::path::Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let diagnostics = diagnostic_content(state);
        if diagnostics.contains("attachment_inaccessible") && diagnostics.contains("missing") {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "live owner did not diagnose the missing transferred attachment"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

struct FailingOwner {
    child: std::process::Child,
    failed: std::path::PathBuf,
    retry: std::path::PathBuf,
    recovered: std::path::PathBuf,
    done: std::path::PathBuf,
}

impl FailingOwner {
    fn start(fixture: &TransferFixture) -> Self {
        let ready = fixture.state().join("failure-ready");
        let failed = fixture.state().join("persistence-failed");
        let retry = fixture.state().join("retry");
        let recovered = fixture.state().join("recovered");
        let done = fixture.state().join("failure-done");
        let child = spawn_failing_owner(fixture, &ready, &failed, &retry, &recovered, &done);
        wait_for_path(&ready);
        wait_for_control_owner(fixture.state(), &fixture.destination);
        Self {
            child,
            failed,
            retry,
            recovered,
            done,
        }
    }

    fn assert_failed(&self) {
        assert!(self.failed.exists(), "persistence failure hook did not run");
    }

    fn retry_and_wait(&self) {
        std::fs::write(&self.retry, b"retry").expect("release exact retry");
        wait_for_path(&self.recovered);
    }

    fn finish(self, fixture: &TransferFixture) {
        fixture.finish_owner(self.child, &self.done);
    }
}

fn spawn_failing_owner(
    fixture: &TransferFixture,
    ready: &std::path::Path,
    failed: &std::path::Path,
    retry: &std::path::Path,
    recovered: &std::path::Path,
    done: &std::path::Path,
) -> std::process::Child {
    let script = r#"
        log_user 0
        set timeout 15
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        close [open $env(PROQI_TEST_READY) w]
        set deadline [expr {[clock milliseconds] + 30000}]
        while {![file exists $env(PROQI_TEST_RETRY)]} {
            if {[clock milliseconds] >= $deadline} { exit 91 }
            expect -timeout 0 {
                -re ".+" { exp_continue }
                timeout {}
                eof { exit 92 }
            }
            after 20
        }
        send -- "r"
        after 800
        close [open $env(PROQI_TEST_RECOVERED) w]
        while {![file exists $env(PROQI_TEST_DONE)]} {
            if {[clock milliseconds] >= $deadline} { exit 93 }
            expect -timeout 0 {
                -re ".+" { exp_continue }
                timeout {}
                eof { exit 94 }
            }
            after 20
        }
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", fixture.binary)
        .env("PROQI_TEST_STATE", fixture.state())
        .env("PROQI_TEST_SESSION", &fixture.destination)
        .env("PROQI_TEST_READY", ready)
        .env("PROQI_TEST_RETRY", retry)
        .env("PROQI_TEST_RECOVERED", recovered)
        .env("PROQI_TEST_DONE", done)
        .env("PROQI_TEST_INPUT_STALL", "1")
        .env("PROQI_TEST_PERSISTENCE_DELAY_MS", "300")
        .env("PROQI_TEST_PERSISTENCE_FAIL_ONCE", "1")
        .env("PROQI_TEST_PERSISTENCE_FAILED", failed)
        .spawn()
        .expect("spawn failing destination owner")
}
