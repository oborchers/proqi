use super::{enforce_support_floor, parse_dependencies, render_config, validate_evidence};
use crate::release_targets::{ALL, Architecture, LibcFamily, OperatingSystem};

use serde_json::json;
use std::{fs, path::Path};

#[test]
fn derived_dependencies_receive_the_declared_support_floor() {
    let derived = parse_dependencies("shlibs:Depends=libc6 (>= 2.34), libgcc-s1 (>= 4.2)\n")
        .expect("dependencies");
    assert_eq!(
        enforce_support_floor(&derived).expect("policy"),
        ["libc6 (>= 2.35)", "libgcc-s1 (>= 4.2)"]
    );
}

#[test]
fn package_config_has_exact_paths_and_no_maintainer_scripts() {
    let config = render_config(
        "0.1.2",
        &[
            "libc6 (>= 2.35)".to_owned(),
            "libgcc-s1 (>= 4.2)".to_owned(),
        ],
        "amd64",
    )
    .expect("config");
    assert!(config.contains("dst: /usr/bin/proqi"));
    assert!(config.contains("dst: /usr/share/doc/proqi/copyright"));
    assert!(!config.contains("scripts:"));
}

#[test]
fn downloaded_evidence_binds_every_artifact_identity() {
    let evidence = json!({
        "schema_version": 2,
        "package": "proqi_amd64.deb",
        "sha256": "package",
        "source_archive": "proqi-linux.tar.gz",
        "source_archive_sha256": "archive",
        "source_binary_sha256": "binary",
    });
    assert!(
        validate_evidence(
            &evidence,
            Some("proqi-linux.tar.gz"),
            "archive",
            "package",
            "binary",
            "proqi_amd64.deb",
        )
        .is_ok()
    );
    for field in ["sha256", "source_archive_sha256", "source_binary_sha256"] {
        let mut tampered = evidence.clone();
        tampered[field] = json!("tampered");
        assert!(
            validate_evidence(
                &tampered,
                Some("proqi-linux.tar.gz"),
                "archive",
                "package",
                "binary",
                "proqi_amd64.deb",
            )
            .is_err()
        );
    }
}

#[test]
fn ci_debian_commands_pass_the_typed_gnu_target() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let ci = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read CI workflow");
    let target = ALL
        .iter()
        .find(|target| {
            target.os == OperatingSystem::Linux
                && target.architecture == Architecture::X86_64
                && target.libc == LibcFamily::Gnu
        })
        .expect("default GNU/Linux release target");
    let package_invocation = [
        format!("target/package/{} \\", target.archive_name()),
        "            target/debian-package \\".to_owned(),
        format!("            {}", target.triple),
    ]
    .join("\n");
    assert!(ci.contains(&package_invocation));

    let verify_invocation = [
        "            target/debian-package/proqi_amd64.deb \\".to_owned(),
        "            target/debian-package \\".to_owned(),
        format!("            {}", target.triple),
    ]
    .join("\n");
    assert!(ci.contains(&verify_invocation));
}
