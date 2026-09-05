//! Read-only doctor and diagnostic independence contracts.

use super::*;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;

#[test]
fn doctor_reports_fresh_state_without_initializing_it() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path().join("state");
    std::fs::create_dir(&root).expect("state root");
    #[cfg(unix)]
    {
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))
            .expect("private state root");
    }
    let report = success(&root, &["doctor"], None);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["overall_status"], "skipped");
    assert!(
        report["checks"]
            .as_array()
            .is_some_and(|checks| checks.iter().any(|check| check["id"] == "sqlite"))
    );
    assert_eq!(
        std::fs::read_dir(&root).expect("read state root").count(),
        0
    );
}

#[cfg(unix)]
#[test]
fn doctor_reports_supported_protocols_and_the_precise_compatibility_boundary() {
    for protocol in [19, 20, 21, 22] {
        let fixture = herdr_fixture::HerdrFixture::new(protocol);
        let temporary = tempfile::tempdir().expect("temporary directory");
        let state_root = temporary.path().join("state");
        std::fs::create_dir(&state_root).expect("state root");
        std::fs::set_permissions(&state_root, std::fs::Permissions::from_mode(0o700))
            .expect("private state root");
        let mut command = Command::new(env!("CARGO_BIN_EXE_proqi"));
        command
            .arg("--state-dir")
            .arg(&state_root)
            .arg("--json")
            .arg("doctor")
            .env("HERDR_ENV", "1")
            .env_remove("PROQI_DISABLE_HERDR")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let fixture_program = fixture.program();
        let fixture_directory = Path::new(&fixture_program)
            .parent()
            .expect("fixture directory")
            .to_path_buf();
        let paths = std::iter::once(fixture_directory).chain(std::env::split_paths(&inherited));
        command.env("PATH", std::env::join_paths(paths).expect("fixture PATH"));
        let output = command.output().expect("run doctor");
        assert!(
            output.status.success(),
            "protocol {protocol}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fixture.prompt_bytes(),
            None,
            "doctor must not send a prompt"
        );
        let envelope: Value = serde_json::from_slice(&output.stdout).expect("doctor JSON");
        let checks = envelope["data"]["checks"]
            .as_array()
            .expect("doctor checks");
        let herdr = checks
            .iter()
            .find(|check| check["id"] == "herdr")
            .expect("Herdr check");
        if protocol <= 21 {
            assert_eq!(herdr["status"], "ok");
            assert_eq!(herdr["facts"]["protocol"], protocol);
            assert_eq!(herdr["facts"]["version"], fixture_version(protocol));
            assert!(herdr.get("remediation").is_none());
        } else {
            assert_eq!(herdr["status"], "warning");
            let remediation = herdr["remediation"].as_str().expect("remediation");
            assert!(
                remediation
                    .contains("qualified protocols 19 through 20, or provisional protocol 21")
            );
            assert!(remediation.contains("protocols 22/22"));
            assert!(remediation.contains("unsupported protocol version"));
        }
    }
}

const fn fixture_version(protocol: u32) -> &'static str {
    match protocol {
        19 => "0.8.0",
        20 => "0.8.2",
        _ => "provisional-fixture",
    }
}

#[test]
fn diagnostics_collection_does_not_open_or_migrate_sqlite() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let data = root.join("data");
    std::fs::create_dir(&data).expect("data directory");
    let database = data.join("proqi.sqlite3");
    let malformed = b"not a sqlite database";
    std::fs::write(&database, malformed).expect("malformed database");
    let output = root.join("support.json");
    let collected = success(
        root,
        &[
            "diagnostics",
            "collect",
            "--output",
            output.to_str().expect("UTF-8 output"),
        ],
        None,
    );
    assert_eq!(collected["bundle_schema_version"], 1);
    assert_eq!(std::fs::read(&database).expect("database bytes"), malformed);
    assert!(!root.join("runtime").exists());
    assert!(!root.join("config").exists());
    assert!(!root.join("cache").exists());
}

#[cfg(unix)]
#[test]
fn doctor_fails_with_stable_json_for_unsafe_configuration() {
    use std::os::unix::fs::symlink;
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let config = root.join("config");
    std::fs::create_dir(&config).expect("config directory");
    let target = root.join("elsewhere.toml");
    std::fs::write(&target, "theme = 'dark'\n").expect("config target");
    symlink(&target, config.join("config.toml")).expect("config symlink");
    let failed = run(root, &["doctor"], None);
    assert!(!failed.status.success());
    let response: Value = serde_json::from_slice(&failed.stdout).expect("error JSON");
    assert_eq!(response["error"]["code"], "doctor_failed");
    assert_eq!(response["error"]["details"]["overall_status"], "fail");
}
