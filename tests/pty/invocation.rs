//! Invocation discovery and completion through a real PTY.

use super::support::{consume_first_run, expect_command, json_command};

fn write_skill(home: &std::path::Path, name: &str) {
    let directory = home.join(".agents/skills").join(name);
    std::fs::create_dir_all(&directory).expect("skill directory");
    std::fs::write(
        directory.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Fixture skill\n---\nbody"),
    )
    .expect("skill definition");
}

fn thought_content(binary: &str, state: &std::path::Path) -> String {
    let sessions = json_command(binary, state, &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    json_command(binary, state, &["thoughts", "list", session])["data"]["thoughts"][0]["content"]
        .as_str()
        .expect("thought content")
        .to_owned()
}

fn write_command(root: &std::path::Path, name: &str) {
    std::fs::create_dir_all(root).expect("command root");
    std::fs::write(
        root.join(format!("{name}.md")),
        "---\ndescription: Fixture command\n---\nbody",
    )
    .expect("command definition");
}

fn select_invocation(
    binary: &str,
    home: &std::path::Path,
    cwd: &std::path::Path,
    query: &str,
) -> String {
    let state = tempfile::tempdir().expect("temporary state");
    consume_first_run(binary, state.path());
    let interact = r#"
        log_user 0
        set timeout 15
        cd $env(PROQI_TEST_CWD)
        set stty_init "rows 14 columns 72"
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        expect -exact "\x1b\[?1049h"
        after 700
        send -- "\x1b\[200~$env(PROQI_TEST_QUERY)\x1b\[201~"
        after 300
        send "\t"
        after 300
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
        .env("PROQI_TEST_CWD", cwd)
        .env("PROQI_TEST_QUERY", query)
        .env("HOME", home)
        .env_remove("HERDR_ENV")
        .status()
        .expect("select invocation in PTY");
    assert!(status.success());
    thought_content(binary, state.path())
}

#[test]
fn short_fuzzy_invocation_completes_exactly_and_shuts_down_in_a_real_pty() {
    let state = tempfile::tempdir().expect("temporary state");
    let home = tempfile::tempdir().expect("isolated home");
    let external = home.path().join("catalog/aos-communication-email");
    let skill = external.join("SKILL.md");
    std::fs::create_dir_all(&external).expect("external skill directory");
    let mut definition =
        "---\nname: aos-communication-email\ndescription: Verfasst E-Mails für Grüße und 界\n---\n"
            .as_bytes()
            .to_vec();
    definition.resize(64 * 1024, 0xff);
    std::fs::write(skill, definition).expect("large skill fixture");
    let agent_root = home.path().join(".agents/skills");
    let claude_root = home.path().join(".claude/skills");
    std::fs::create_dir_all(&agent_root).expect("Agent Skills root");
    std::fs::create_dir_all(&claude_root).expect("Claude skills root");
    let agent_alias = agent_root.join("aos-communication-email");
    std::os::unix::fs::symlink(&external, &agent_alias).expect("Agent Skills alias");
    std::os::unix::fs::symlink(&agent_alias, claude_root.join("aos-communication-email"))
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
        send -- "\x1b\[200~\$aos-ce\x1b\[201~"
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
        "$aos-communication-email "
    );
}

#[test]
fn incomplete_depth_discovery_is_visible_in_a_real_pty() {
    let state = tempfile::tempdir().expect("temporary state");
    let home = tempfile::tempdir().expect("isolated home");
    let project = tempfile::tempdir().expect("isolated project");
    std::fs::create_dir_all(project.path().join(".git")).expect("repository marker");
    let commands = project.path().join(".opencode/commands");
    std::fs::create_dir_all(&commands).expect("command root");
    std::fs::write(
        commands.join("visible.md"),
        "---\ndescription: Visible command\n---\nbody",
    )
    .expect("visible command");
    let hidden = (1..=7).fold(commands, |path, index| path.join(format!("d{index}")));
    std::fs::create_dir_all(&hidden).expect("deep command directory");
    std::fs::write(
        hidden.join("hidden.md"),
        "---\ndescription: Hidden command\n---\nbody",
    )
    .expect("deep command");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let interact = r#"
        log_user 0
        set timeout 10
        cd $env(PROQI_TEST_PROJECT)
        set stty_init "rows 12 columns 72"
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        expect -exact "\x1b\[?1049h"
        after 500
        send -- "\x1b\[200~/v\x1b\[201~"
        expect -glob "*incomplete results, refine query*"
        send "\t"
        after 300
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
        .env("PROQI_TEST_PROJECT", project.path())
        .env("HOME", home.path())
        .env_remove("HERDR_ENV")
        .status()
        .expect("run incomplete invocation PTY workflow");
    assert!(status.success());
    assert_eq!(thought_content(binary, state.path()), "/visible ");
}

#[test]
fn result_beyond_twenty_is_selectable_by_keyboard_and_mouse_in_real_ptys() {
    let home = tempfile::tempdir().expect("isolated home");
    for index in 0..25 {
        write_skill(home.path(), &format!("skill{index:02}-zoom"));
    }
    let binary = env!("CARGO_BIN_EXE_proqi");
    let keyboard_state = tempfile::tempdir().expect("keyboard state");
    consume_first_run(binary, keyboard_state.path());
    let keyboard = r#"
        log_user 0
        set timeout 10
        set stty_init "rows 10 columns 56"
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        expect -exact "\x1b\[?1049h"
        after 500
        send -- "\x1b\[200~\$sz\x1b\[201~"
        expect -glob "*more results exist, refine query*"
        for {set i 0} {$i < 22} {incr i} {
            send -- "\x1b\[B"
            after 20
        }
        send "\t"
        after 300
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let keyboard_status = expect_command()
        .args(["-c", keyboard])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", keyboard_state.path())
        .env("HOME", home.path())
        .env_remove("HERDR_ENV")
        .status()
        .expect("run keyboard invocation PTY workflow");
    assert!(keyboard_status.success());
    assert_eq!(
        thought_content(binary, keyboard_state.path()),
        "$skill22-zoom "
    );

    let mouse_state = tempfile::tempdir().expect("mouse state");
    consume_first_run(binary, mouse_state.path());
    let mouse = r#"
        log_user 0
        set timeout 10
        set stty_init "rows 10 columns 56"
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        expect -exact "\x1b\[?1049h"
        after 500
        send -- "\x1b\[200~\$sz\x1b\[201~"
        expect -glob "*more results exist, refine query*"
        for {set i 0} {$i < 22} {incr i} {
            send -- "\x1b\[<65;8;8M"
            after 20
        }
        after 300
        send -- "\x1b\[<0;3;9M\x1b\[<0;3;9m"
        after 300
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let mouse_status = expect_command()
        .args(["-c", mouse])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", mouse_state.path())
        .env("HOME", home.path())
        .env_remove("HERDR_ENV")
        .status()
        .expect("run mouse invocation PTY workflow");
    assert!(mouse_status.success());
    assert_eq!(
        thought_content(binary, mouse_state.path()),
        "$skill22-zoom "
    );
}

#[test]
fn former_plugin_and_ancestor_ceilings_are_selectable_in_real_ptys() {
    let fixture = tempfile::tempdir().expect("fixture");
    let home = fixture.path().join("home");
    let repo = fixture.path().join("repo");
    std::fs::create_dir_all(repo.join(".git")).expect("repository marker");
    write_command(&repo.join(".claude/commands"), "root-command");
    let cwd = (0..20).fold(repo, |path, index| path.join(format!("d{index}")));
    std::fs::create_dir_all(&cwd).expect("deep cwd");

    let mut plugins = serde_json::Map::new();
    for index in 0..65 {
        let root = fixture.path().join(format!("plugins/p{index:03}"));
        write_command(&root.join("commands"), "item");
        plugins.insert(
            format!("p{index:03}@fixture"),
            serde_json::json!([{"installPath":root}]),
        );
    }
    let multi = fixture.path().join("plugins/multi");
    plugins.insert(
        "multi@fixture".to_owned(),
        serde_json::Value::Array(
            (0..5)
                .map(|index| {
                    let installation = multi.join(format!("install-{index}"));
                    write_command(&installation.join("commands"), &format!("install-{index}"));
                    serde_json::json!({"installPath":installation})
                })
                .collect(),
        ),
    );
    let components = fixture.path().join("plugins/components");
    let component_paths = (0..17)
        .map(|index| format!("commands-{index:02}"))
        .collect::<Vec<_>>();
    std::fs::create_dir_all(components.join(".claude-plugin")).expect("manifest root");
    std::fs::write(
        components.join(".claude-plugin/plugin.json"),
        serde_json::json!({"name":"components","commands":component_paths}).to_string(),
    )
    .expect("component manifest");
    for index in 0..17 {
        write_command(
            &components.join(format!("commands-{index:02}")),
            &format!("item-{index:02}"),
        );
    }
    plugins.insert(
        "components@fixture".to_owned(),
        serde_json::json!([{"installPath":components}]),
    );
    std::fs::create_dir_all(home.join(".claude/plugins")).expect("registry root");
    std::fs::write(
        home.join(".claude/plugins/installed_plugins.json"),
        serde_json::json!({"plugins":plugins}).to_string(),
    )
    .expect("plugin registry");

    let binary = env!("CARGO_BIN_EXE_proqi");
    assert_eq!(
        select_invocation(binary, &home, &cwd, "/p064:item"),
        "/p064:item "
    );
    assert_eq!(
        select_invocation(binary, &home, &cwd, "/multi:install-4"),
        "/multi:install-4 "
    );
    assert_eq!(
        select_invocation(binary, &home, &cwd, "/components:item-16"),
        "/components:item-16 "
    );
    assert_eq!(
        select_invocation(binary, &home, &cwd, "/root-command"),
        "/root-command "
    );
}
