//! Durable terminal-safe boundary and relative insertion workflows in a real PTY.

use super::support::{expect_command, json_command, json_input_command};

fn create_session(binary: &str, state: &std::path::Path, thoughts: &[&str]) -> String {
    let created = json_command(binary, state, &[]);
    let session = created["data"]["session_id"]
        .as_str()
        .expect("session ID")
        .to_owned();
    for thought in thoughts {
        let _added = json_input_command(binary, state, &["thoughts", "add", &session], thought);
    }
    session
}

fn run(binary: &str, state: &std::path::Path, session: &str, script: &str) {
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .env_remove("HERDR_ENV")
        .status()
        .expect("run terminal-safe PTY workflow");
    assert!(status.success(), "terminal-safe PTY failed: {status}");
}

fn contents(binary: &str, state: &std::path::Path, session: &str) -> Vec<String> {
    json_command(binary, state, &["thoughts", "list", session])["data"]["thoughts"]
        .as_array()
        .expect("thought list")
        .iter()
        .map(|thought| {
            thought["content"]
                .as_str()
                .expect("thought content")
                .to_owned()
        })
        .collect()
}

#[test]
fn control_boundaries_and_alt_insertions_survive_resize_undo_redo_and_restart() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let session = create_session(
        binary,
        state.path(),
        &["first Grüße", "second 界", "third 👩‍💻", "fourth", "last"],
    );
    let workflow = r#"
        log_user 0
        set timeout 15
        set stty_init "rows 6 columns 22"
        spawn /bin/sh -c {before=$(stty -g); "$PROQI_TEST_BINARY" --state-dir "$PROQI_TEST_STATE" -r "$PROQI_TEST_SESSION"; result=$?; after=$(stty -g); [ "$before" = "$after" ] || exit 90; exit "$result"}
        expect -exact "\x1b\[?1049h"
        after 300
        send -- "\x1b\[1;5B"
        send -- "\x1b\[1;3A\x1b\[1;3A"
        send -- "\x1b\[200~above last\x1b\[201~"
        after 300
        send "\x1b"
        stty rows 18 columns 76
        after 150
        send -- "\x1b\[107;5u"
        send -- "\x1b\[106;3u"
        send -- "\x1b\[200~below first\x1b\[201~"
        after 300
        send "\x1b"
        after 150
        send "u"
        after 250
        send -- "\x1b\[90;10u"
        after 600
        send -- $env(PROQI_TEST_PRIMARY_Q)
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    run(binary, state.path(), &session, workflow);
    let expected = [
        "first Grüße",
        "below first",
        "second 界",
        "third 👩‍💻",
        "fourth",
        "above last",
        "last",
    ];
    assert_eq!(contents(binary, state.path(), &session), expected);

    let restart = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        after 300
        send -- "\x1b\[1;5A\x1b\[1;5B"
        send -- $env(PROQI_TEST_PRIMARY_Q)
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    run(binary, state.path(), &session, restart);
    assert_eq!(contents(binary, state.path(), &session), expected);
}

#[test]
fn control_edit_endpoints_and_shifted_selection_persist_exact_content() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let original = "alpha\r\n\tGrüße 界 e\u{301} 👩‍💻\u{7}\nomega";
    let session = create_session(binary, state.path(), &[original]);
    let endpoints = r#"
        log_user 0
        set timeout 12
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        after 300
        send "\r"
        send -- "\x1b\[1;5A^\x1b\[1;5B"
        send -- {$}
        after 500
        send "\x1b"
        after 150
        send -- $env(PROQI_TEST_PRIMARY_Q)
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    run(binary, state.path(), &session, endpoints);
    let decorated = format!("^{original}$");
    assert_eq!(
        contents(binary, state.path(), &session),
        [decorated.as_str()]
    );

    let selection = r#"
        log_user 0
        set timeout 12
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        after 300
        send "\r"
        send -- "\x1b\[1;5B\x1b\[1;6A"
        send -- "\x1b\[200~replacement 界\x1b\[201~"
        after 400
        send "\x1b"
        after 150
        send -- $env(PROQI_TEST_PRIMARY_Q)
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    run(binary, state.path(), &session, selection);
    assert_eq!(contents(binary, state.path(), &session), ["replacement 界"]);

    run(
        binary,
        state.path(),
        &session,
        r#"
            log_user 0
            set timeout 10
            spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
            expect -exact "\x1b\[?1049h"
            after 300
            send -- $env(PROQI_TEST_PRIMARY_Q)
            expect eof
            catch wait result
            exit [lindex $result 3]
        "#,
    );
    assert_eq!(contents(binary, state.path(), &session), ["replacement 界"]);
}

#[test]
fn control_shift_board_boundary_selects_every_live_thought_but_not_insertion() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let session = create_session(binary, state.path(), &["first", "middle", "last"]);
    run(
        binary,
        state.path(),
        &session,
        r#"
            log_user 0
            set timeout 10
            spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
            expect -exact "\x1b\[?1049h"
            after 300
            send -- "\x1b\[1;5A\x1b\[1;6B\x1b\[1;6B"
            send "d"
            after 400
            send "\x1b"
            after 150
            send -- $env(PROQI_TEST_PRIMARY_Q)
            expect eof
            catch wait result
            exit [lindex $result 3]
        "#,
    );
    assert!(contents(binary, state.path(), &session).is_empty());
}
