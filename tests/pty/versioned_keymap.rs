//! Exact terminal bytes must produce durable actions through configured aliases.

use super::support::{consume_first_run, expect_command, json_command};

fn run(state: &std::path::Path, session: &str, sequence: &str) {
    let script = r#"
        log_user 0
        set timeout 10
        set args [list $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)]
        if {$env(PROQI_TEST_SESSION) ne ""} { lappend args -r $env(PROQI_TEST_SESSION) }
        spawn {*}$args
        expect -exact "\x1b\[?1049h"
        after 300
        if {$env(PROQI_TEST_SESSION) ne ""} { send "\x1b"; after 100 }
        send -- $env(PROQI_TEST_KEYS)
        after 500
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", env!("CARGO_BIN_EXE_proqi"))
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_KEYS", sequence)
        .status()
        .expect("configured action PTY");
    assert!(status.success(), "configured action exited with {status}");
}

#[test]
fn aliases_replace_defaults_and_survive_undo_restart_and_config_reload() {
    let state = tempfile::tempdir().unwrap();
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let config = state.path().join("config/config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    let original = "check_for_updates=false\n";
    let map = "\n[keymap]\nschema_version=1\n[keymap.bindings.board]\n\"thought.delete\"=[{key='F5'},{key='F6'}]\n\"history.undo\"=[{key='F7'}]\n[keymap.bindings.insertion_boundary]\n\"history.undo\"=[{key='F7'}]\n";
    std::fs::write(&config, format!("{original}{map}")).unwrap();
    let content = "Grüße 界 e\u{301}\tcontrol\nsecond";
    run(state.path(), "", &format!("\x1b[200~{content}\x1b[201~"));
    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"].as_str().unwrap();
    for (keys, expected) in [
        ("d\x1b[3~", Some(content)),
        ("\x1b[15~", None),
        ("\x1b[18~", Some(content)),
        ("\x1b[17~", None),
        ("\x1b[18~", Some(content)),
    ] {
        run(state.path(), session, keys);
        let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
        let actual = thoughts["data"]["items"].as_array().unwrap();
        assert_eq!(
            actual.len(),
            usize::from(expected.is_some()),
            "keys={keys:?}"
        );
        if let Some(expected) = expected {
            assert_eq!(actual[0]["content"], expected);
        }
    }
    std::fs::write(
        &config,
        format!(
            "{original}{}",
            map.replace("F5", "F9").replace(",{key='F6'}", "")
        ),
    )
    .unwrap();
    run(state.path(), session, "\x1b[15~\x1b[17~");
    let retained = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(retained["data"]["items"][0]["content"], content);
    run(state.path(), session, "\x1b[20~");
    let deleted = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert!(deleted["data"]["items"].as_array().unwrap().is_empty());
}
