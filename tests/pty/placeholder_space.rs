//! Exact placeholder-aware Space durability and undo through a real PTY.

use super::support::{consume_first_run, expect_command, json_command};

use proqi::domain::{ContentAnnotation, ContentAnnotationKind};
use rusqlite::Connection;

#[test]
fn selected_file_placeholder_moves_right_and_undoes_in_a_real_pty() {
    let state = tempfile::tempdir().expect("temporary state");
    let files = tempfile::tempdir().expect("temporary files");
    let file = files.path().join("placeholder-space-界.png");
    std::fs::write(&file, b"fixture image bytes").expect("file fixture");
    let binary = env!("CARGO_BIN_EXE_proqi");

    consume_first_run(binary, state.path());
    create_placeholder(binary, state.path(), &file);
    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    let thought = thoughts["data"]["thoughts"][0]["id"]
        .as_str()
        .expect("thought ID");
    let original = file.to_string_lossy();
    assert_durable_value(state.path(), original.as_ref(), 0, original.len());

    type_space_before_selected_placeholder(binary, state.path(), session);
    let moved = json_command(
        binary,
        state.path(),
        &["thoughts", "inspect", session, thought],
    );
    assert_eq!(moved["data"]["thought"]["content"], format!(" {original}"));
    assert_durable_value(state.path(), &format!(" {original}"), 1, original.len() + 1);

    undo_after_restart(binary, state.path(), session);
    let undone = json_command(
        binary,
        state.path(),
        &["thoughts", "inspect", session, thought],
    );
    assert_eq!(undone["data"]["thought"]["content"], original.as_ref());
    assert_durable_value(state.path(), original.as_ref(), 0, original.len());
}

fn create_placeholder(binary: &str, state: &std::path::Path, file: &std::path::Path) {
    let script = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        set dropped $env(PROQI_TEST_DROP)
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        send -- "\x1b\[200~$dropped\x1b\[201~"
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
        .env("PROQI_TEST_DROP", file)
        .status()
        .expect("create placeholder in PTY");
    assert!(status.success());
}

fn type_space_before_selected_placeholder(binary: &str, state: &std::path::Path, session: &str) {
    let script = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        set session $env(PROQI_TEST_SESSION)
        spawn $binary --state-dir $state -r $session
        expect -exact "\x1b\[?1049h"
        send "\r"
        after 100
        send "\x01"
        after 100
        send -- " "
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
        .status()
        .expect("type placeholder-aware Space in PTY");
    assert!(status.success());
}

fn undo_after_restart(binary: &str, state: &std::path::Path, session: &str) {
    let script = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        set session $env(PROQI_TEST_SESSION)
        spawn $binary --state-dir $state -r $session
        expect -exact "\x1b\[?1049h"
        send "\r"
        after 100
        send "\x1a"
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
        .status()
        .expect("undo placeholder Space after restart in PTY");
    assert!(status.success());
}

fn assert_durable_value(state: &std::path::Path, content: &str, start: usize, end: usize) {
    let database = state.join("data/proqi.sqlite3");
    let connection = Connection::open(database).expect("open durable state");
    let (stored_content, annotations): (String, String) = connection
        .query_row(
            "SELECT content, annotations_json FROM thoughts WHERE deleted_at IS NULL",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("load durable placeholder");
    let annotations: Vec<ContentAnnotation> =
        serde_json::from_str(&annotations).expect("parse annotations");
    assert_eq!(stored_content, content);
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].start, start);
    assert_eq!(annotations[0].end, end);
    assert!(matches!(
        annotations[0].kind,
        ContentAnnotationKind::Attachment { image: true, .. }
    ));
}
