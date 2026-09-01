//! First-run PTY behavior, durable copy, ordinary interactions, and once-only eligibility.

use std::collections::BTreeSet;

use proqi::{
    adapters::memory::FakeIdGenerator,
    application::{FirstRunEnvironment, first_run_board},
    domain::{ContentAnnotation, Session, SessionId, ThoughtId, Timestamp},
    ports::environment::IdGenerator as _,
};

use super::support::{expect_command, json_command};

fn only_session(binary: &str, state: &std::path::Path) -> String {
    let sessions = json_command(binary, state, &["sessions", "list"]);
    let values = sessions["data"]["sessions"].as_array().expect("sessions");
    assert_eq!(values.len(), 1);
    values[0]["id"].as_str().expect("session ID").to_owned()
}

fn contents(binary: &str, state: &std::path::Path, session: &str) -> Vec<String> {
    json_command(binary, state, &["thoughts", "list", session])["data"]["thoughts"]
        .as_array()
        .expect("thoughts")
        .iter()
        .map(|thought| {
            thought["content"]
                .as_str()
                .expect("thought content")
                .to_owned()
        })
        .collect()
}

fn session_ids(binary: &str, state: &std::path::Path) -> BTreeSet<String> {
    json_command(binary, state, &["sessions", "list"])["data"]["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .map(|session| session["id"].as_str().expect("session ID").to_owned())
        .collect()
}

fn expected_contents(environment: FirstRunEnvironment) -> Vec<String> {
    expected_board(environment)
        .live_thoughts()
        .into_iter()
        .map(|thought| thought.content.clone())
        .collect()
}

fn expected_annotations(environment: FirstRunEnvironment) -> Vec<Vec<ContentAnnotation>> {
    expected_board(environment)
        .live_thoughts()
        .into_iter()
        .map(|thought| thought.annotations.clone())
        .collect()
}

fn expected_board(environment: FirstRunEnvironment) -> proqi::domain::SessionBoard {
    let mut ids = FakeIdGenerator::new(1_725_300_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-pty-onboarding-oracle"),
        Timestamp::from_millis(1),
    )
    .expect("oracle session");
    first_run_board(session, &mut ids, environment)
        .expect("oracle practice board")
        .board()
        .clone()
}

fn durable_annotations(state: &std::path::Path) -> Vec<Vec<ContentAnnotation>> {
    let connection =
        rusqlite::Connection::open(state.join("data/proqi.sqlite3")).expect("onboarding database");
    let mut statement = connection
        .prepare("SELECT annotations_json FROM thoughts WHERE deleted_at IS NULL ORDER BY position")
        .expect("annotation query");
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("annotation rows")
        .map(|row| {
            serde_json::from_str(&row.expect("annotation JSON")).expect("valid durable annotations")
        })
        .collect()
}

fn run_launch(
    binary: &str,
    state: &std::path::Path,
    script: &str,
    session: Option<&str>,
    expectation: &str,
) {
    let mut command = expect_command();
    command
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env_remove("HERDR_ENV");
    if let Some(session) = session {
        command.env("PROQI_TEST_SESSION", session);
    }
    assert!(command.status().expect(expectation).success());
}

#[test]
fn managed_and_unmanaged_fresh_pty_launches_persist_exact_distinct_copy() {
    for (managed, environment) in [
        (false, FirstRunEnvironment::Standalone),
        (true, FirstRunEnvironment::HerdrManaged),
    ] {
        let state = tempfile::tempdir().expect("temporary state");
        let binary = env!("CARGO_BIN_EXE_proqi");
        let script = r#"
            log_user 0
            set timeout 10
            spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
            expect -exact "\x1b\[?1049h"
            after 500
            send "q"
            expect eof
            catch wait result
            exit [lindex $result 3]
        "#;
        let mut command = expect_command();
        command
            .args(["-c", script])
            .env("PROQI_TEST_BINARY", binary)
            .env("PROQI_TEST_STATE", state.path());
        if managed {
            command.env("HERDR_ENV", "1");
        } else {
            command.env_remove("HERDR_ENV");
        }
        let status = command.status().expect("run first eligible PTY launch");
        assert!(status.success());
        let session = only_session(binary, state.path());
        assert_eq!(
            contents(binary, state.path(), &session),
            expected_contents(environment)
        );
        assert_eq!(
            durable_annotations(state.path()),
            expected_annotations(environment)
        );
    }
}

#[test]
fn pristine_session_browser_neither_seeds_nor_consumes_interactive_eligibility() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let browser = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r
        expect -exact "\x1b\[?1049h"
        after 400
        send "\x1b"
        expect {
            eof {}
            timeout {exit 124}
        }
        catch wait result
        exit [lindex $result 3]
    "#;
    run_launch(binary, state.path(), browser, None, "pristine browser");
    assert!(session_ids(binary, state.path()).is_empty());

    let fresh = browser
        .replace(" -r", "")
        .replace("send \"\\x1b\"", "send \"q\"");
    run_launch(binary, state.path(), &fresh, None, "eligible fresh launch");
    let tutorial = only_session(binary, state.path());
    assert_eq!(contents(binary, state.path(), &tutorial).len(), 6);
}

#[test]
fn tutorial_shortcut_annotations_survive_cli_cross_session_transfer() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let first_launch = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        expect -exact "\x1b\[?1049h"
        after 500
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    run_launch(
        binary,
        state.path(),
        first_launch,
        None,
        "seed tutorial for transfer",
    );
    let source = only_session(binary, state.path());
    let listed = json_command(binary, state.path(), &["thoughts", "list", &source]);
    let source_thought = &listed["data"]["thoughts"][1];
    let source_thought_id = source_thought["id"].as_str().expect("thought ID");
    let destination = json_command(binary, state.path(), &[])["data"]["session_id"]
        .as_str()
        .expect("destination session ID")
        .to_owned();
    let transferred = json_command(
        binary,
        state.path(),
        &["thoughts", "send", &source, source_thought_id, &destination],
    );
    let destination_thought = transferred["data"]["destination_thought_id"]
        .as_str()
        .expect("destination thought ID");

    let expected_board = expected_board(FirstRunEnvironment::Standalone);
    let expected = expected_board.live_thoughts()[1];
    let destination_bytes = destination
        .parse::<SessionId>()
        .expect("destination session ID")
        .database_bytes();
    let thought_bytes = destination_thought
        .parse::<ThoughtId>()
        .expect("destination thought ID")
        .database_bytes();
    let connection =
        rusqlite::Connection::open(state.path().join("data/proqi.sqlite3")).expect("reload store");
    let (content, annotations_json): (String, String) = connection
        .query_row(
            "SELECT content, annotations_json FROM thoughts \
             WHERE session_id = ?1 AND id = ?2 AND deleted_at IS NULL",
            rusqlite::params![destination_bytes.as_slice(), thought_bytes.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("transferred durable thought");
    let annotations: Vec<ContentAnnotation> =
        serde_json::from_str(&annotations_json).expect("transferred annotations");
    assert_eq!(content, expected.content);
    assert_eq!(annotations, expected.annotations);
    assert!(content.contains("Primary+Shift+U"));
}

#[test]
fn delete_undo_resume_continue_and_later_fresh_launches_never_reseed() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let first = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        expect -exact "\x1b\[?1049h"
        after 500
        send "a"
        after 200
        send "d"
        after 700
        send "\x1b"
        after 100
        send "u"
        after 700
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    run_launch(binary, state.path(), first, None, "delete and undo");
    let tutorial = only_session(binary, state.path());
    assert_eq!(contents(binary, state.path(), &tutorial).len(), 6);

    let empty = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        after 500
        send "ad"
        after 500
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    run_launch(binary, state.path(), empty, Some(&tutorial), "empty board");
    assert!(contents(binary, state.path(), &tutorial).is_empty());

    let resume = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        after 500
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    run_launch(
        binary,
        state.path(),
        resume,
        Some(&tutorial),
        "resume empty practice board",
    );
    assert!(contents(binary, state.path(), &tutorial).is_empty());

    let continue_latest = resume.replace("-r $env(PROQI_TEST_SESSION)", "-c");
    run_launch(
        binary,
        state.path(),
        &continue_latest,
        None,
        "continue empty practice board",
    );
    assert!(contents(binary, state.path(), &tutorial).is_empty());

    let fresh = resume.replace("-r $env(PROQI_TEST_SESSION)", "");

    for _ in 0..2 {
        let before = session_ids(binary, state.path());
        run_launch(binary, state.path(), &fresh, None, "later fresh launch");
        let after = session_ids(binary, state.path());
        let created = after.difference(&before).collect::<Vec<_>>();
        assert_eq!(created.len(), 1);
        assert!(contents(binary, state.path(), created[0]).is_empty());
    }
}

#[test]
fn practice_board_uses_existing_navigation_edit_create_and_paste_paths() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let script = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        expect -exact "\x1b\[?1049h"
        stty rows 14 columns 50
        after 400
        send "jk"
        send -- "\x1b\[B\x1b\[A"
        send "\r"
        after 200
        send "!"
        after 100
        send "\x1b"
        after 200
        send "a"
        after 200
        send "d"
        after 700
        send "\x1b"
        after 100
        send "u"
        after 700
        send -- "\x1b\[200~Grüße 界\nsecond\x1b\[201~"
        after 200
        send "\x1b"
        after 200
        send "n"
        after 200
        send "created normally"
        after 100
        send "\x1b"
        after 200
        stty rows 6 columns 22
        after 150
        stty rows 24 columns 100
        after 300
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env_remove("HERDR_ENV")
        .status()
        .expect("practice-board interactions");
    assert!(status.success());
    let session = only_session(binary, state.path());
    let persisted = contents(binary, state.path(), &session);
    assert_eq!(persisted.len(), 8, "persisted thoughts: {persisted:?}");
    assert!(persisted[..6].iter().any(|content| content.contains('!')));
    assert!(
        persisted
            .iter()
            .any(|content| content == "Grüße 界\nsecond")
    );
    assert!(
        persisted
            .iter()
            .any(|content| content == "created normally")
    );
}
