use super::{enforce_support_floor, parse_dependencies, render_config, validate_evidence};

use serde_json::json;

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
