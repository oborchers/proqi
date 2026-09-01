//! Installed-product contract exercised only by `cargo xtask package`.

use std::{
    env, fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::Arc,
};

use rusqlite::Connection;
use serde_json::Value;

#[cfg(unix)]
#[path = "package_contract/pty.rs"]
mod pty;
#[path = "package_contract/sandbox.rs"]
mod sandbox;

use sandbox::PackageSandbox;

struct InstalledProduct {
    binary: PathBuf,
    archive: PathBuf,
    state: PathBuf,
    working: PathBuf,
    sandbox: Arc<PackageSandbox>,
}

impl InstalledProduct {
    fn from_environment() -> Self {
        let sandbox = Arc::new(
            PackageSandbox::create(&required_path("PROQI_PACKAGE_ROOT"))
                .expect("create package-contract sandbox"),
        );
        Self {
            binary: required_path("PROQI_PACKAGE_BINARY"),
            archive: required_path("PROQI_PACKAGE_ARCHIVE"),
            state: sandbox.state().to_owned(),
            working: sandbox.working().to_owned(),
            sandbox,
        }
    }

    fn command(&self) -> Command {
        isolated_command(&self.binary, &self.working)
    }

    fn state_command(&self) -> Command {
        let mut command = self.command();
        command.arg("--state-dir").arg(&self.state);
        command
    }

    fn json(&self, arguments: &[&str]) -> Value {
        let mut command = self.state_command();
        command.arg("--json").args(arguments);
        parse_success(&command.output().expect("run installed JSON command"))
    }

    fn json_input(&self, arguments: &[&str], input: &str) -> Value {
        let mut command = self.state_command();
        command
            .arg("--json")
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        let mut child = command.spawn().expect("spawn installed input command");
        child
            .stdin
            .take()
            .expect("input pipe")
            .write_all(input.as_bytes())
            .expect("write exact input");
        parse_success(&child.wait_with_output().expect("wait for input command"))
    }
}

#[test]
#[ignore = "run by cargo xtask package with an installed release binary"]
fn installed_product_contract() {
    let product = InstalledProduct::from_environment();
    let owned_root = product.sandbox.root().to_owned();
    assert!(!product.state.join("data/proqi.sqlite3").exists());
    assert_identity_and_completion_contract(&product);
    let (session, thought, content) = assert_json_workflow(&product);
    assert_reopen_and_resume(&product, &session, &thought, &content);
    assert_migration_and_newer_schema_contract(&product);
    assert_archive_and_runtime_independence(&product);
    #[cfg(unix)]
    pty::assert_active_owner_and_terminal_restoration(&product, &session);
    drop(product);
    assert!(!owned_root.exists(), "package sandbox survived its owner");
}

fn assert_identity_and_completion_contract(product: &InstalledProduct) {
    let version = product
        .command()
        .arg("--version")
        .output()
        .expect("installed version");
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout),
        format!("proqi {}\n", env!("CARGO_PKG_VERSION"))
    );
    let help = product
        .command()
        .arg("--help")
        .output()
        .expect("installed help");
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("completions"));
    for shell in ["bash", "zsh", "fish"] {
        let generated = product
            .command()
            .args(["completions", shell])
            .output()
            .expect("installed completion");
        assert!(generated.status.success(), "{shell} completion failed");
        assert!(generated.stdout.len() > 100);
    }
}

fn assert_json_workflow(product: &InstalledProduct) -> (String, String, String) {
    let created = product.json(&[]);
    let session = json_string(&created, &["data", "session_id"]);
    assert!(session.starts_with("ses_"));
    let content = "\tGrüße e\u{301} 界\n\n  preserved whitespace  \r\n".to_owned();
    let added = product.json_input(&["thoughts", "add", &session], &content);
    let thought = json_string(&added, &["data", "thought_id"]);
    assert!(thought.starts_with("tht_"));
    let inspected = product.json(&["thoughts", "inspect", &session, &thought]);
    assert_eq!(
        value_at(&inspected, &["data", "thought", "content"]),
        &content
    );
    (session, thought, content)
}

fn assert_reopen_and_resume(
    product: &InstalledProduct,
    session: &str,
    thought: &str,
    content: &str,
) {
    let resumed = product.json(&["-r", session]);
    assert_eq!(value_at(&resumed, &["data", "session_id"]), session);
    let reopened = product.json(&["thoughts", "inspect", session, thought]);
    assert_eq!(
        value_at(&reopened, &["data", "thought", "content"]),
        content
    );
    let listed = product.json(&["sessions", "list"]);
    assert!(
        value_at(&listed, &["data", "sessions"])
            .as_array()
            .is_some_and(|sessions| sessions.iter().any(|item| item["id"] == session))
    );
}

fn assert_migration_and_newer_schema_contract(product: &InstalledProduct) {
    let migration = product.state.join("migration");
    let migration_db = migration.join("data/proqi.sqlite3");
    fs::create_dir_all(migration_db.parent().expect("migration parent"))
        .expect("create migration state");
    Connection::open(&migration_db)
        .expect("legacy database")
        .execute_batch("CREATE TABLE legacy(value TEXT); INSERT INTO legacy VALUES ('kept');")
        .expect("legacy schema");
    let migrated = run_for_state(product, &migration, &["sessions", "list"]);
    assert!(migrated.status.success(), "migration failed: {migrated:?}");
    let backup = only_file(&migration.join("data/backups"));
    let backup_connection = Connection::open(backup).expect("migration backup");
    let kept: String = backup_connection
        .query_row("SELECT value FROM legacy", [], |row| row.get(0))
        .expect("backup content");
    assert_eq!(kept, "kept");
    let integrity: String = Connection::open(&migration_db)
        .expect("migrated database")
        .pragma_query_value(None, "quick_check", |row| row.get(0))
        .expect("quick check");
    assert_eq!(integrity, "ok");

    let newer = product.state.join("newer");
    let newer_db = newer.join("data/proqi.sqlite3");
    fs::create_dir_all(newer_db.parent().expect("newer parent")).expect("create newer state");
    Connection::open(&newer_db)
        .expect("newer database")
        .execute_batch(
            "CREATE TABLE schema_meta(singleton INTEGER, schema_version INTEGER);\n\
             INSERT INTO schema_meta VALUES (1, 99);",
        )
        .expect("newer schema");
    let before = fs::read(&newer_db).expect("newer bytes before");
    let refused = run_for_state(product, &newer, &["sessions", "list"]);
    assert!(!refused.status.success());
    let error: Value = serde_json::from_slice(&refused.stdout).expect("newer-schema JSON");
    assert_eq!(value_at(&error, &["error", "code"]), "unsupported");
    assert_eq!(fs::read(newer_db).expect("newer bytes after"), before);
}

fn assert_archive_and_runtime_independence(product: &InstalledProduct) {
    assert!(product.archive.is_file());
    assert!(
        fs::metadata(&product.archive)
            .expect("archive metadata")
            .len()
            > 1_000
    );
    let output = product
        .command()
        .env_clear()
        .env("PROQI_DISABLE_HERDR", "1")
        .args(["--json", "capabilities"])
        .output()
        .expect("runtime-independent capabilities");
    assert!(output.status.success(), "native binary failed: {output:?}");
    let capabilities: Value = serde_json::from_slice(&output.stdout).expect("capability JSON");
    assert!(
        value_at(&capabilities, &["data", "commands"])
            .as_array()
            .is_some_and(|commands| commands.iter().any(|command| command == "sessions"))
    );
}

fn package_version() -> proqi::domain::StableVersion {
    proqi::domain::StableVersion::parse(env!("CARGO_PKG_VERSION"))
        .expect("canonical package version")
}

fn run_for_state(product: &InstalledProduct, state: &Path, arguments: &[&str]) -> Output {
    let mut command = product.command();
    command
        .arg("--state-dir")
        .arg(state)
        .arg("--json")
        .args(arguments)
        .output()
        .expect("run isolated state command")
}

fn isolated_command(binary: &Path, working: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .env_clear()
        .current_dir(working)
        .env("PROQI_DISABLE_HERDR", "1")
        .env("NO_PROXY", "*")
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1");
    command
}

fn parse_success(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "installed command failed: {output:?}"
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("installed command JSON");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["ok"], true);
    value
}

fn json_string(value: &Value, path: &[&str]) -> String {
    value_at(value, path)
        .as_str()
        .expect("JSON string")
        .to_owned()
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> &'a Value {
    path.iter().fold(value, |current, key| &current[*key])
}

fn required_path(name: &str) -> PathBuf {
    env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| panic!("{name} must contain an absolute path"))
}

fn only_file(directory: &Path) -> PathBuf {
    let files = fs::read_dir(directory)
        .expect("read directory")
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    assert_eq!(
        files.len(),
        1,
        "expected one file in {}",
        directory.display()
    );
    files[0].clone()
}
