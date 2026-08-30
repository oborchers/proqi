use std::collections::BTreeSet;

use proqi::application::FirstRunEnvironment;

use super::{expect_command, json_command};

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
            environment.thought_contents().map(str::to_owned)
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
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) sessions
        expect -exact "\x1b\[?1049h"
        after 400
        catch {send "q"}
        catch {expect eof}
        catch wait result
        exit [lindex $result 3]
    "#;
    run_launch(binary, state.path(), browser, None, "pristine browser");
    assert!(session_ids(binary, state.path()).is_empty());

    let fresh = browser.replace(" sessions", "");
    run_launch(binary, state.path(), &fresh, None, "eligible fresh launch");
    let tutorial = only_session(binary, state.path());
    assert_eq!(contents(binary, state.path(), &tutorial).len(), 6);
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
        send "ad"
        after 500
        send "u"
        after 500
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
        send "ad"
        after 300
        send "u"
        after 300
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
