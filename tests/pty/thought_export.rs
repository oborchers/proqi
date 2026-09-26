//! Real-terminal plain-text export: path field, completion, confirmation, and failures.
//!
//! Every scenario runs in a disposable working directory, binds the three export
//! actions to F5, F6, and F7 through versioned configuration, records the complete
//! terminal transcript with named checkpoints, and proves each step from the exact
//! rendered screen plus the written file bytes and the durable Board.

use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt as _};

use super::support::{consume_first_run, expect_command, json_command, json_input_command};

const ROWS: u16 = 20;
const COLUMNS: u16 = 90;
const KEYMAP: &str = "check_for_updates=false\n[keymap]\nschema_version=1\n[keymap.bindings.board]\n\"thought.export\"=[{key='F5'}]\n\"thought.export_remove\"=[{key='F6'}]\n\"thought.export_replace\"=[{key='F7'}]\n";

/// Expect harness: transcript capture, named checkpoints, and durable file waits.
const PROLOGUE: &str = r#"
    log_user 0
    set timeout 10
    match_max -d 1048576
    set stty_init "rows 20 columns 90"
    set capture [open $env(PROQI_TEST_TRANSCRIPT) w]
    fconfigure $capture -translation binary -encoding utf-8
    set marks [open $env(PROQI_TEST_CHECKPOINTS) w]
    proc drain {} {
        global capture
        expect -timeout 0 -re {.+} {
            puts -nonewline $capture $expect_out(buffer)
            exp_continue
        } timeout {} eof {}
    }
    proc pause {milliseconds} {
        for {set elapsed 0} {$elapsed < $milliseconds} {incr elapsed 10} {
            drain
            after 10
        }
        drain
    }
    proc checkpoint {name} {
        global capture marks
        pause 400
        flush $capture
        puts $marks "$name [tell $capture]"
        flush $marks
    }
    proc wait_file {path} {
        for {set waits 0} {$waits < 500} {incr waits} {
            if {[file exists $path]} { return }
            pause 20
        }
        exit 90
    }
    proc quit {} {
        global capture
        send "\x1b"
        pause 100
        send "q"
        expect {
            -re {.+} {
                puts -nonewline $capture $expect_out(buffer)
                exp_continue
            }
            eof {}
            timeout { exit 92 }
        }
        close $capture
        catch wait result
        exit [lindex $result 3]
    }
    cd $env(PROQI_TEST_WORK)
    spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
    expect -exact "\x1b\[?1049h"
    puts -nonewline $capture "\x1b\[?1049h"
    pause 300
    send "\x1b"
    pause 100
"#;

struct Scenario {
    state: tempfile::TempDir,
    work: tempfile::TempDir,
    session: String,
}

impl Scenario {
    fn new(bodies: &[&str]) -> Self {
        let state = tempfile::tempdir().expect("state");
        let work = tempfile::tempdir().expect("work directory");
        let binary = env!("CARGO_BIN_EXE_proqi");
        consume_first_run(binary, state.path());
        let config = state.path().join("config/config.toml");
        fs::create_dir_all(config.parent().expect("config parent")).expect("config dir");
        fs::write(&config, KEYMAP).expect("keymap");
        let created = json_command(
            binary,
            state.path(),
            &["sessions", "create", "--name", "export"],
        );
        let session = created["data"]["session_id"]
            .as_str()
            .expect("session")
            .to_owned();
        for body in bodies {
            json_input_command(binary, state.path(), &["thoughts", "add", &session], body);
        }
        Self {
            state,
            work,
            session,
        }
    }

    /// Run one step script and return the rendered screen at every named checkpoint.
    fn run(&self, steps: &str, environment: &[(&str, &str)]) -> BTreeMap<String, String> {
        let transcript = self.state.path().join("export.transcript");
        let checkpoints = self.state.path().join("export.checkpoints");
        let mut command = expect_command();
        command
            .args(["-c", &format!("{PROLOGUE}\n{steps}\nquit\n")])
            .env("PROQI_TEST_BINARY", env!("CARGO_BIN_EXE_proqi"))
            .env("PROQI_TEST_STATE", self.state.path())
            .env("PROQI_TEST_SESSION", &self.session)
            .env("PROQI_TEST_WORK", self.work.path())
            .env("PROQI_TEST_TRANSCRIPT", &transcript)
            .env("PROQI_TEST_CHECKPOINTS", &checkpoints)
            .env("HOME", self.work.path())
            .env_remove("COLORTERM")
            .env_remove("HERDR_ENV");
        for (key, value) in environment {
            command.env(key, value);
        }
        let status = command.status().expect("run export PTY scenario");
        assert!(status.success(), "export PTY scenario exited with {status}");
        let bytes = fs::read(transcript).expect("transcript");
        fs::read_to_string(checkpoints)
            .expect("checkpoints")
            .lines()
            .map(|line| {
                let (name, offset) = line.rsplit_once(' ').expect("checkpoint line");
                let offset = offset.parse::<usize>().expect("offset").min(bytes.len());
                let mut parser = vt100::Parser::new(ROWS, COLUMNS, 0);
                parser.process(&bytes[..offset]);
                (name.to_owned(), parser.screen().contents())
            })
            .collect()
    }

    fn contents(&self) -> Vec<String> {
        json_command(
            env!("CARGO_BIN_EXE_proqi"),
            self.state.path(),
            &["thoughts", "list", &self.session],
        )["data"]["items"]
            .as_array()
            .expect("items")
            .iter()
            .map(|item| item["content"].as_str().expect("content").to_owned())
            .collect()
    }

    fn undo(&self) {
        json_command(
            env!("CARGO_BIN_EXE_proqi"),
            self.state.path(),
            &["thoughts", "undo", &self.session],
        );
    }

    fn path(&self, relative: &str) -> std::path::PathBuf {
        self.work.path().join(relative)
    }
}

/// Assert that each named checkpoint screen shows every expected fragment.
fn assert_screens(screens: &BTreeMap<String, String>, expected: &[(&str, &[&str])]) {
    for (name, fragments) in expected {
        let screen = screens
            .get(*name)
            .unwrap_or_else(|| panic!("missing checkpoint {name}"));
        for fragment in *fragments {
            assert!(
                screen.contains(fragment),
                "checkpoint {name} lacks {fragment:?}:\n{screen}"
            );
        }
    }
}

#[test]
fn keep_completes_the_path_with_tab_and_arrows_and_writes_exact_copy_text() {
    let scenario = Scenario::new(&["first Grüße 👩‍💻\r\n", "\tsecond"]);
    fs::create_dir(scenario.path("docs")).expect("docs");
    fs::create_dir(scenario.path("drafts")).expect("drafts");
    let screens = scenario.run(
        r#"
        send "a"
        pause 100
        send -- "\x1b\[15~"
        checkpoint opened
        send -- $env(PROQI_TEST_PRIMARY_A)
        send -- "d"
        send "\t"
        checkpoint listed
        send "\t"
        checkpoint cycled
        send -- "\x1b\[B"
        checkpoint arrow_down
        send -- "\x1b\[A"
        checkpoint arrow_up
        send -- "\x1b\[Z"
        checkpoint reversed
        send -- "out.txt\r"
        wait_file "$env(PROQI_TEST_WORK)/docs/out.txt"
        checkpoint saved
        "#,
        &[],
    );
    assert_screens(
        &screens,
        &[
            (
                "opened",
                &["export to file", "Save 2 thoughts as plain text", ".txt"],
            ),
            ("listed", &[">docs/", "drafts/"]),
            ("cycled", &[">drafts/"]),
            ("arrow_down", &[">docs/"]),
            ("arrow_up", &[">drafts/"]),
            ("reversed", &[">docs/"]),
            ("saved", &["exported 2 thoughts to out.txt"]),
        ],
    );
    assert_eq!(
        fs::read(scenario.path("docs/out.txt")).expect("export"),
        "first Grüße 👩‍💻\r\n\n\n\tsecond".as_bytes()
    );
    assert_eq!(scenario.contents(), ["first Grüße 👩‍💻\r\n", "\tsecond"]);
}

#[test]
fn remove_confirms_replacement_with_cancel_first_and_undo_keeps_the_file() {
    let scenario = Scenario::new(&["remove me"]);
    fs::write(scenario.path("taken.txt"), "old").expect("existing");
    let screens = scenario.run(
        r#"
        send -- "\x1b\[17~"
        checkpoint opened
        send -- $env(PROQI_TEST_PRIMARY_A)
        send -- "taken.txt\r"
        checkpoint confirm
        send "\r"
        checkpoint kept
        send "\r"
        checkpoint again
        send -- "\x1b\[B"
        send "\r"
        checkpoint removed
        "#,
        &[],
    );
    assert_screens(
        &screens,
        &[
            (
                "opened",
                &["export and remove", "Save and remove 1 thought"],
            ),
            (
                "confirm",
                &["replace existing file?", "› Cancel", "Replace taken.txt"],
            ),
            ("kept", &["existing file kept", ">taken.txt"]),
            ("again", &["replace existing file?"]),
            (
                "removed",
                &["exported 1 thought to taken.txt and removed it"],
            ),
        ],
    );
    assert!(scenario.contents().is_empty());
    assert_eq!(
        fs::read_to_string(scenario.path("taken.txt")).expect("file"),
        "remove me"
    );
    scenario.undo();
    assert_eq!(scenario.contents(), ["remove me"]);
    assert_eq!(
        fs::read_to_string(scenario.path("taken.txt")).expect("file"),
        "remove me",
        "undo keeps the file"
    );
}

#[test]
fn replace_from_commands_uses_the_default_session_path_and_one_undo_step() {
    let scenario = Scenario::new(&["first", "second"]);
    let screens = scenario.run(
        r#"
        send "a"
        pause 100
        send ":"
        pause 200
        send -- "export to file and replace"
        checkpoint commands
        send "\r"
        checkpoint opened
        send "\r"
        checkpoint replaced
        "#,
        &[],
    );
    assert_screens(
        &screens,
        &[
            (
                "commands",
                &["Export to file and replace with reference..."],
            ),
            ("opened", &["export and replace with reference", "export-"]),
            ("replaced", &["[File 1]"]),
        ],
    );
    let written = fs::read_dir(scenario.work.path())
        .expect("work directory")
        .map(|entry| entry.expect("entry").path())
        .collect::<Vec<_>>();
    assert_eq!(written.len(), 1, "{written:?}");
    let name = written[0]
        .file_name()
        .and_then(|name| name.to_str())
        .expect("name");
    assert!(
        name.starts_with("export-")
            && written[0]
                .extension()
                .and_then(|extension| extension.to_str())
                == Some("txt"),
        "{name}"
    );
    assert_eq!(
        fs::read_to_string(&written[0]).expect("file"),
        "first\n\nsecond"
    );
    let directory = fs::canonicalize(scenario.work.path()).expect("canonical");
    assert_eq!(
        scenario.contents(),
        [format!("{}/{name} ", directory.display())]
    );
    scenario.undo();
    assert_eq!(scenario.contents(), ["first", "second"]);
    assert!(written[0].exists(), "undo keeps the file");
}

#[test]
fn read_only_missing_and_changed_destinations_never_change_the_board() {
    let scenario = Scenario::new(&["keep me"]);
    let locked = scenario.path("locked");
    fs::create_dir(&locked).expect("locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).expect("read-only");
    fs::write(scenario.path("changing.txt"), "before").expect("existing");
    let screens = scenario.run(
        r#"
        send -- "\x1b\[18~"
        pause 200
        send -- $env(PROQI_TEST_PRIMARY_A)
        send -- "locked/x.txt\r"
        checkpoint denied
        send -- $env(PROQI_TEST_PRIMARY_A)
        send -- "missing/x.txt\r"
        checkpoint missing
        send -- $env(PROQI_TEST_PRIMARY_A)
        send -- "changing.txt\r"
        checkpoint confirm
        set handle [open "$env(PROQI_TEST_WORK)/changing.txt" w]
        puts -nonewline $handle "changed during confirmation"
        close $handle
        send -- "\x1b\[B"
        send "\r"
        checkpoint changed
        send "\x1b"
        checkpoint cancelled
        "#,
        &[],
    );
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("restore");
    assert_screens(
        &screens,
        &[
            ("denied", &["permission denied", "board was not changed"]),
            ("missing", &["folder does not exist"]),
            ("confirm", &["replace existing file?"]),
            (
                "changed",
                &["file changed after confirmation", ">changing.txt"],
            ),
            ("cancelled", &["export cancelled"]),
        ],
    );
    assert_eq!(scenario.contents(), ["keep me"]);
    assert_eq!(fs::read_dir(&locked).expect("locked").count(), 0);
    assert!(!scenario.path("missing").exists());
    assert_eq!(
        fs::read_to_string(scenario.path("changing.txt")).expect("file"),
        "changed during confirmation"
    );
}

#[test]
fn a_full_disk_fails_the_write_and_keeps_the_thoughts() {
    let scenario = Scenario::new(&["keep me too"]);
    let screens = scenario.run(
        r#"
        send -- "\x1b\[17~"
        pause 200
        send -- $env(PROQI_TEST_PRIMARY_A)
        send -- "full.txt\r"
        checkpoint full
        "#,
        &[
            ("PROQI_TEST_INPUT_STALL", "1"),
            ("PROQI_TEST_EXPORT_FAILURE", "storage_full"),
        ],
    );
    assert_screens(&screens, &[("full", &["disk is full", ">full.txt"])]);
    assert_eq!(scenario.contents(), ["keep me too"]);
    assert_eq!(
        fs::read_dir(scenario.work.path()).expect("work").count(),
        0,
        "no destination or temporary file remains"
    );
}

#[test]
fn pointer_rows_cancel_save_and_confirm_replacement() {
    let scenario = Scenario::new(&["mouse body"]);
    fs::write(scenario.path("taken.txt"), "old").expect("existing");
    // At 90x20 the five-row overlay spans rows 6-10 and columns 17-74 (1-based SGR
    // coordinates): close is at column 73, row 6, and rows 8 and 9 are the first
    // and second choices. Transient status reuses the footer gap row. Clicks are
    // spaced beyond the multi-click window so none is treated as a repeat.
    let screens = scenario.run(
        r#"
        send -- "\x1b\[15~"
        pause 300
        send -- "\x1b\[<0;73;6M\x1b\[<0;73;6m"
        checkpoint closed
        pause 300
        send -- "\x1b\[17~"
        pause 200
        send -- $env(PROQI_TEST_PRIMARY_A)
        send -- "taken.txt"
        pause 700
        send -- "\x1b\[<0;20;8M\x1b\[<0;20;8m"
        checkpoint confirm
        pause 300
        send -- "\x1b\[<0;20;8M\x1b\[<0;20;8m"
        checkpoint kept
        pause 300
        send -- "\x1b\[<0;20;8M\x1b\[<0;20;8m"
        checkpoint again
        pause 300
        send -- "\x1b\[<0;20;9M\x1b\[<0;20;9m"
        pause 500
        checkpoint replaced
        "#,
        &[],
    );
    assert_screens(
        &screens,
        &[
            ("closed", &["export cancelled"]),
            ("confirm", &["replace existing file?"]),
            ("kept", &["existing file kept"]),
            ("again", &["replace existing file?"]),
            (
                "replaced",
                &["exported 1 thought to taken.txt and removed it"],
            ),
        ],
    );
    assert!(
        scenario.contents().is_empty(),
        "pointer replacement removed the thought"
    );
    assert_eq!(
        fs::read_to_string(scenario.path("taken.txt")).expect("file"),
        "mouse body"
    );
    let entries = fs::read_dir(scenario.work.path()).expect("work").count();
    assert_eq!(entries, 1, "the cancelled export wrote nothing");
}
