use super::{expect_command, json_command};

#[test]
fn escaped_unicode_file_drop_becomes_one_durable_absolute_path() {
    let state = tempfile::tempdir().expect("temporary state");
    let files = tempfile::tempdir().expect("temporary files");
    let file = files.path().join("Grüße 第一 sample.png");
    std::fs::write(&file, b"fixture image bytes").expect("file fixture");
    let escaped = file.to_string_lossy().replace(' ', "\\ ");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let create = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        set dropped $env(PROQI_TEST_DROP)
        spawn $binary --state-dir $state
        after 500
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
        .args(["-c", create])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_DROP", escaped)
        .status()
        .expect("run PTY path-drop workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        thoughts["data"]["thoughts"][0]["content"],
        file.to_string_lossy().as_ref()
    );
}
