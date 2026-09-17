//! Cross-session transfer into a live owner with real annotations and durable storage.

use proqi::{
    adapters::runtime::SystemIdGenerator,
    domain::{ContentAnnotation, ContentAnnotationKind, SessionId, ThoughtId},
    ports::environment::IdGenerator,
};

use super::support::{
    expect_command, json_command, json_input_command, raw_input_command, wait_for_control_owner,
    wait_for_path,
};

#[test]
fn annotated_transfer_into_active_owner_is_durable_and_keeps_sequences_contiguous() {
    let fixture = TransferFixture::new("Transferred Grüße 第一 image.png");
    fixture.seed_destination_attachment("Existing destination image.png");
    let (owner, done) = fixture.start_owner("copy");
    let operation = SystemIdGenerator.operation_id().to_string();
    let transferred = fixture.send(&operation);
    let transferred_id = assert_initial_transfer(&fixture, &transferred, &operation);
    assert_replay_and_conflict(&fixture, &operation);
    assert_followup_write(&fixture);
    fixture.finish_owner(owner, &done);
    assert_copy_after_restart(&fixture, &transferred_id);
}

fn assert_initial_transfer(
    fixture: &TransferFixture,
    output: &std::process::Output,
    operation: &str,
) -> String {
    assert!(
        output.status.success(),
        "active annotated transfer failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let transferred: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("transfer JSON");
    let transferred_id = transferred["data"]["destination_thought_id"]
        .as_str()
        .expect("destination thought ID")
        .to_owned();
    assert_eq!(transferred["data"]["destination_receipt"]["sequence"], 2);
    assert_eq!(
        transferred["data"]["destination_receipt"]["identity"]["id"],
        operation
    );
    assert_eq!(
        transferred["data"]["destination_receipt"]["idempotent_replay"],
        false
    );
    assert_destination_database(
        fixture.state(),
        &fixture.destination,
        &transferred_id,
        fixture.image.to_string_lossy().as_ref(),
        (2, 2, 2, 2),
    );
    transferred_id
}

fn assert_replay_and_conflict(fixture: &TransferFixture, operation: &str) {
    let replay = fixture.send(operation);
    assert!(replay.status.success());
    let replay: serde_json::Value = serde_json::from_slice(&replay.stdout).expect("replay JSON");
    assert_eq!(replay["data"]["destination_receipt"]["sequence"], 2);
    assert_eq!(
        replay["data"]["destination_receipt"]["idempotent_replay"],
        true
    );

    let conflicting_source = json_input_command(
        fixture.binary,
        fixture.state(),
        &["thoughts", "add", &fixture.source],
        "conflicting transfer payload",
    );
    let conflicting_source = conflicting_source["data"]["thought_id"]
        .as_str()
        .expect("conflicting source ID");
    let conflict = raw_input_command(
        fixture.binary,
        fixture.state(),
        &[
            "thoughts",
            "send",
            &fixture.source,
            conflicting_source,
            &fixture.destination,
            "--operation-id",
            operation,
        ],
        "",
    );
    assert!(!conflict.status.success());
    let conflict: serde_json::Value =
        serde_json::from_slice(&conflict.stdout).expect("conflict JSON");
    assert_eq!(conflict["error"]["code"], "idempotency_conflict");
}

fn assert_followup_write(fixture: &TransferFixture) {
    let next_operation = SystemIdGenerator.operation_id().to_string();
    let next = raw_input_command(
        fixture.binary,
        fixture.state(),
        &[
            "thoughts",
            "add",
            &fixture.destination,
            "--operation-id",
            &next_operation,
        ],
        "ordinary write after transfer",
    );
    assert!(
        next.status.success(),
        "write after active annotated transfer failed: {}",
        String::from_utf8_lossy(&next.stdout)
    );
    let next: serde_json::Value = serde_json::from_slice(&next.stdout).expect("next write JSON");
    assert_eq!(next["data"]["receipt"]["sequence"], 3);
}

fn assert_copy_after_restart(fixture: &TransferFixture, transferred_id: &str) {
    let destination_thoughts = json_command(
        fixture.binary,
        fixture.state(),
        &["thoughts", "list", &fixture.destination],
    );
    let live = destination_thoughts["data"]["thoughts"]
        .as_array()
        .expect("destination thoughts");
    assert_eq!(live.len(), 3);
    assert_eq!(
        live[0]["content"],
        fixture.seed_image().to_string_lossy().as_ref()
    );
    assert_eq!(live[1]["content"], fixture.image.to_string_lossy().as_ref());
    assert_eq!(live[2]["content"], "ordinary write after transfer");
    let source_after = json_command(
        fixture.binary,
        fixture.state(),
        &["thoughts", "list", &fixture.source],
    );
    assert_eq!(
        source_after["data"]["thoughts"]
            .as_array()
            .expect("source thoughts")
            .len(),
        2,
        "copy transfer must retain both source thoughts"
    );
    assert_destination_database(
        fixture.state(),
        &fixture.destination,
        transferred_id,
        fixture.image.to_string_lossy().as_ref(),
        (3, 3, 3, 2),
    );
    assert_history_round_trip(fixture, transferred_id);
}

fn assert_history_round_trip(fixture: &TransferFixture, transferred_id: &str) {
    move_board_history(fixture, "undo");
    move_board_history(fixture, "undo");
    let empty = json_command(
        fixture.binary,
        fixture.state(),
        &["thoughts", "list", &fixture.destination],
    );
    let after_undo = empty["data"]["thoughts"]
        .as_array()
        .expect("thoughts after undo");
    assert_eq!(after_undo.len(), 1);
    assert_eq!(
        after_undo[0]["content"],
        fixture.seed_image().to_string_lossy().as_ref()
    );
    move_board_history(fixture, "redo");
    assert_destination_database(
        fixture.state(),
        &fixture.destination,
        transferred_id,
        fixture.image.to_string_lossy().as_ref(),
        (6, 3, 6, 2),
    );
    move_board_history(fixture, "redo");
    let restored = json_command(
        fixture.binary,
        fixture.state(),
        &["thoughts", "list", &fixture.destination],
    );
    assert_eq!(
        restored["data"]["thoughts"][2]["content"],
        "ordinary write after transfer"
    );
    assert_destination_database(
        fixture.state(),
        &fixture.destination,
        transferred_id,
        fixture.image.to_string_lossy().as_ref(),
        (7, 3, 7, 2),
    );
}

fn move_board_history(fixture: &TransferFixture, direction: &str) {
    let operation = SystemIdGenerator.operation_id().to_string();
    json_command(
        fixture.binary,
        fixture.state(),
        &[
            "thoughts",
            direction,
            &fixture.destination,
            "--operation-id",
            &operation,
        ],
    );
}

pub(super) struct TransferFixture {
    state: tempfile::TempDir,
    files: tempfile::TempDir,
    pub(super) image: std::path::PathBuf,
    pub(super) binary: &'static str,
    pub(super) source: String,
    pub(super) destination: String,
    pub(super) source_thought: String,
    seed_image: std::cell::OnceCell<std::path::PathBuf>,
}

impl TransferFixture {
    pub(super) fn new(image_name: &str) -> Self {
        let state = tempfile::tempdir().expect("temporary state");
        let files = tempfile::tempdir().expect("temporary files");
        let image = files.path().join(image_name);
        std::fs::write(&image, b"fixture image bytes").expect("image fixture");
        let binary = env!("CARGO_BIN_EXE_proqi");
        let source = new_session(binary, state.path());
        let destination = new_session(binary, state.path());
        create_annotated_source(binary, state.path(), &source, &image);
        let source_list = json_command(binary, state.path(), &["thoughts", "list", &source]);
        let source_thought = source_list["data"]["thoughts"][0]["id"]
            .as_str()
            .expect("source thought ID")
            .to_owned();
        Self {
            state,
            files,
            image,
            binary,
            source,
            destination,
            source_thought,
            seed_image: std::cell::OnceCell::new(),
        }
    }

    fn seed_destination_attachment(&self, image_name: &str) {
        let image = self.files.path().join(image_name);
        std::fs::write(&image, b"existing image bytes").expect("destination image fixture");
        create_annotated_source(self.binary, self.state(), &self.destination, &image);
        self.seed_image.set(image).expect("one destination seed");
    }

    fn seed_image(&self) -> &std::path::Path {
        self.seed_image.get().expect("destination seed image")
    }

    pub(super) fn state(&self) -> &std::path::Path {
        self.state.path()
    }

    pub(super) fn send(&self, operation: &str) -> std::process::Output {
        raw_input_command(
            self.binary,
            self.state(),
            &[
                "thoughts",
                "send",
                &self.source,
                &self.source_thought,
                &self.destination,
                "--operation-id",
                operation,
            ],
            "",
        )
    }

    pub(super) fn start_owner(&self, name: &str) -> (std::process::Child, std::path::PathBuf) {
        let ready = self.state().join(format!("{name}-ready"));
        let done = self.state().join(format!("{name}-done"));
        let owner = spawn_owner(self.binary, self.state(), &self.destination, &ready, &done);
        wait_for_path(&ready);
        wait_for_control_owner(self.state(), &self.destination);
        (owner, done)
    }

    pub(super) fn finish_owner(&self, mut owner: std::process::Child, done: &std::path::Path) {
        std::fs::write(done, b"done").expect("release destination owner");
        let status = owner.wait().expect("wait for destination owner");
        assert!(status.success(), "destination owner exited with {status}");
        restart_session(self.binary, self.state(), &self.destination);
    }
}

pub(super) fn new_session(binary: &str, state: &std::path::Path) -> String {
    json_command(binary, state, &[])["data"]["session_id"]
        .as_str()
        .expect("session ID")
        .to_owned()
}

pub(super) fn create_annotated_source(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    image: &std::path::Path,
) {
    let escaped = image
        .to_string_lossy()
        .replace(' ', "\\ ")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let script = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        send -- "\x1b\[200~$env(PROQI_TEST_DROP)\x1b\[201~"
        after 700
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_DROP", escaped)
        .status()
        .expect("create annotated source thought");
    assert!(status.success(), "source PTY exited with {status}");
}

pub(super) fn spawn_owner(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    ready: &std::path::Path,
    done: &std::path::Path,
) -> std::process::Child {
    let script = r#"
        log_user 0
        set timeout 12
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        close [open $env(PROQI_TEST_READY) w]
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
    expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_READY", ready)
        .env("PROQI_TEST_DONE", done)
        .spawn()
        .expect("spawn destination owner")
}

pub(super) fn restart_session(binary: &str, state: &std::path::Path, session: &str) {
    let script = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        after 400
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .status()
        .expect("restart exact destination session");
    assert!(
        status.success(),
        "restarted destination exited with {status}"
    );
}

pub(super) fn assert_destination_database(
    state: &std::path::Path,
    session: &str,
    thought: &str,
    content: &str,
    expected: (i64, i64, i64, u64),
) {
    let (expected_sequence, expected_operations, expected_receipts, expected_ordinal) = expected;
    let session = session.parse::<SessionId>().expect("session ID");
    let thought = thought.parse::<ThoughtId>().expect("thought ID");
    let connection =
        rusqlite::Connection::open(state.join("data/proqi.sqlite3")).expect("destination DB");
    let (stored_content, annotations_json): (String, String) = connection
        .query_row(
            "SELECT content, annotations_json FROM thoughts \
             WHERE session_id = ?1 AND id = ?2 AND deleted_at IS NULL",
            rusqlite::params![
                session.database_bytes().as_slice(),
                thought.database_bytes().as_slice()
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("durable transferred thought");
    assert_eq!(stored_content, content);
    let annotations: Vec<ContentAnnotation> =
        serde_json::from_str(&annotations_json).expect("durable annotations");
    assert_eq!(annotations.len(), 1);
    assert_eq!(
        (annotations[0].start, annotations[0].end),
        (0, content.len())
    );
    let ContentAnnotationKind::Attachment {
        ordinal,
        image,
        display_name,
    } = &annotations[0].kind
    else {
        panic!("transferred annotation must remain an attachment");
    };
    assert!(*image);
    assert_eq!(
        ordinal.expect("destination ordinal").get(),
        expected_ordinal
    );
    let expected_name = std::path::Path::new(content)
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .expect("attachment display name");
    assert_eq!(display_name, expected_name);
    let (sequence, image_high): (i64, i64) = connection
        .query_row(
            "SELECT last_durable_sequence, attachment_image_high FROM sessions WHERE id = ?1",
            [session.database_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("destination session metadata");
    assert_eq!(sequence, expected_sequence);
    assert_eq!(
        image_high,
        i64::try_from(expected_ordinal).expect("image high")
    );
    let operations: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM board_operations WHERE session_id = ?1",
            [session.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .expect("destination operations");
    let receipts: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM commit_receipts WHERE session_id = ?1",
            [session.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .expect("destination receipts");
    assert_eq!(operations, expected_operations);
    assert_eq!(receipts, expected_receipts);
}
