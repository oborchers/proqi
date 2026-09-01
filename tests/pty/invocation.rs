//! Invocation discovery and completion through a real PTY.

use super::support::{consume_first_run, expect_command, json_command};

#[test]
fn discovered_invocation_completes_and_shuts_down_in_a_real_pty() {
    let state = tempfile::tempdir().expect("temporary state");
    let home = tempfile::tempdir().expect("isolated home");
    let external = home.path().join("catalog/aos-media-image");
    let skill = external.join("SKILL.md");
    std::fs::create_dir_all(&external).expect("external skill directory");
    std::fs::write(
        skill,
        "---\nname: aos-media-image\ndescription: Erstellt Bilder für Grüße und 界\n---\nfixture body",
    )
    .expect("skill fixture");
    let agent_root = home.path().join(".agents/skills");
    let claude_root = home.path().join(".claude/skills");
    std::fs::create_dir_all(&agent_root).expect("Agent Skills root");
    std::fs::create_dir_all(&claude_root).expect("Claude skills root");
    let agent_alias = agent_root.join("aos-media-image");
    std::os::unix::fs::symlink(&external, &agent_alias).expect("Agent Skills alias");
    std::os::unix::fs::symlink(&agent_alias, claude_root.join("aos-media-image"))
        .expect("Claude skill alias");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let interact = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        after 500
        send -- "\x1b\[200~\$aos\x1b\[201~"
        after 300
        send "\t"
        after 700
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", interact])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("HOME", home.path())
        .env_remove("HERDR_ENV")
        .status()
        .expect("run invocation PTY workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        thoughts["data"]["thoughts"][0]["content"],
        "$aos-media-image "
    );
}
