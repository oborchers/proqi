//! Immutable release-candidate selection and manifest ownership.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

const WORKFLOW: &str = ".github/workflows/release-candidate.yml";
const CI_WORKFLOW: &str = ".github/workflows/ci.yml";

pub(super) fn select(root: &Path, tag: &str, sha: &str, index: &Path) -> Result<(), String> {
    validate_tag_sha(tag, sha)?;
    let index = resolve(root, index);
    let value: Value = serde_json::from_slice(
        &fs::read(&index).map_err(|error| format!("read {}: {error}", index.display()))?,
    )
    .map_err(|error| format!("parse candidate index: {error}"))?;
    let selected = select_value(&value, tag, sha)?;
    println!("{selected}");
    Ok(())
}

pub(super) fn manifest_command(root: &Path, operation: &str) -> Result<(), String> {
    let arguments = std::env::args().skip(3).collect::<Vec<_>>();
    match (operation, arguments.as_slice()) {
        ("create", [directory, tag, source_ref, sha, run_id, run_attempt]) => {
            create_manifest(
                &resolve(root, Path::new(directory)),
                tag,
                source_ref,
                sha,
                parse_u64(run_id, "run ID")?,
                parse_u64(run_attempt, "run attempt")?,
            )
        }
        ("verify", [directory, tag, sha, run_id, run_attempt]) => verify_manifest(
            &resolve(root, Path::new(directory)),
            tag,
            sha,
            parse_u64(run_id, "run ID")?,
            parse_u64(run_attempt, "run attempt")?,
        ),
        _ => Err("candidate-manifest expects `create <dir> <tag> <source-ref> <sha> <run-id> <run-attempt>` or `verify <dir> <tag> <sha> <run-id> <run-attempt>`".to_owned()),
    }
}

fn select_value(index: &Value, tag: &str, sha: &str) -> Result<Value, String> {
    if index.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err("candidate index requires schema_version 1".to_owned());
    }
    let ci = index
        .get("required_ci")
        .and_then(Value::as_array)
        .ok_or_else(|| "candidate index has no required_ci array".to_owned())?;
    let successful_ci = ci
        .iter()
        .filter(|run| {
            string(run, "workflow_path") == Some(CI_WORKFLOW)
                && string(run, "event") == Some("push")
                && string(run, "head_branch") == Some("main")
                && string(run, "head_sha") == Some(sha)
                && string(run, "conclusion") == Some("success")
        })
        .count();
    if successful_ci != 1 {
        return Err(format!(
            "expected one successful required main CI run for {sha}, found {successful_ci}"
        ));
    }

    let expected_name = artifact_name(tag, sha);
    let candidates = index
        .get("candidates")
        .and_then(Value::as_array)
        .ok_or_else(|| "candidate index has no candidates array".to_owned())?;
    let eligible = candidates
        .iter()
        .filter(|run| eligible_candidate(run, tag, sha, &expected_name))
        .collect::<Vec<_>>();
    if eligible.len() != 1 {
        return Err(format!(
            "expected exactly one eligible unexpired candidate `{expected_name}` for {sha}, found {}",
            eligible.len()
        ));
    }
    let run = eligible[0];
    let artifact = run
        .get("artifact")
        .ok_or_else(|| "eligible run has no artifact".to_owned())?;
    let digest = string(artifact, "digest")
        .filter(|digest| valid_digest(digest))
        .ok_or_else(|| "candidate artifact has no valid sha256 digest".to_owned())?;
    Ok(json!({
        "schema_version": 1,
        "run_id": integer(run, "run_id")?,
        "run_attempt": integer(run, "run_attempt")?,
        "artifact_id": integer(artifact, "id")?,
        "artifact_name": expected_name,
        "artifact_digest": digest,
        "source_ref": string(run, "source_ref").ok_or_else(|| "candidate has no source_ref".to_owned())?,
        "source_sha": sha,
        "tag": tag,
    }))
}

fn eligible_candidate(run: &Value, tag: &str, sha: &str, name: &str) -> bool {
    let source_ref = string(run, "source_ref");
    let source_is_allowed =
        source_ref == Some("refs/heads/main") || source_ref == Some(&format!("refs/tags/{tag}"));
    let artifact = run.get("artifact");
    string(run, "workflow_path") == Some(WORKFLOW)
        && matches!(string(run, "event"), Some("push" | "workflow_dispatch"))
        && string(run, "head_sha") == Some(sha)
        && string(run, "conclusion") == Some("success")
        && source_is_allowed
        && artifact.and_then(|value| string(value, "name")) == Some(name)
        && artifact
            .and_then(|value| value.get("expired"))
            .and_then(Value::as_bool)
            == Some(false)
}

fn create_manifest(
    directory: &Path,
    tag: &str,
    source_ref: &str,
    sha: &str,
    run_id: u64,
    run_attempt: u64,
) -> Result<(), String> {
    validate_tag_sha(tag, sha)?;
    validate_source_ref(source_ref, tag)?;
    let release_files = records(directory, &release_file_names())?;
    let evidence_files = records(directory, &evidence_file_names(tag)?)?;
    reject_unlisted(directory, &release_files, &evidence_files)?;
    let manifest = json!({
        "schema_version": 4,
        "version": tag.trim_start_matches('v'),
        "tag": tag,
        "source_ref": source_ref,
        "source_sha": sha,
        "build_run_id": run_id,
        "build_run_attempt": run_attempt,
        "workflow": WORKFLOW,
        "candidate_artifact": artifact_name(tag, sha),
        "targets": super::release_targets::ALL,
        "release_files": release_files,
        "evidence_files": evidence_files,
    });
    fs::write(
        directory.join("candidate-manifest.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&manifest)
                .map_err(|error| format!("render manifest: {error}"))?
        ),
    )
    .map_err(|error| format!("write candidate manifest: {error}"))
}

fn verify_manifest(
    directory: &Path,
    tag: &str,
    sha: &str,
    run_id: u64,
    run_attempt: u64,
) -> Result<(), String> {
    let path = directory.join("candidate-manifest.json");
    let value: Value = serde_json::from_slice(
        &fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?,
    )
    .map_err(|error| format!("parse candidate manifest: {error}"))?;
    let expected_targets = json!(super::release_targets::ALL);
    let valid_header = value.get("schema_version").and_then(Value::as_u64) == Some(4)
        && string(&value, "version") == tag.strip_prefix('v')
        && string(&value, "tag") == Some(tag)
        && string(&value, "source_sha") == Some(sha)
        && value.get("build_run_id").and_then(Value::as_u64) == Some(run_id)
        && value.get("build_run_attempt").and_then(Value::as_u64) == Some(run_attempt)
        && string(&value, "workflow") == Some(WORKFLOW)
        && string(&value, "candidate_artifact") == Some(&artifact_name(tag, sha))
        && value.get("targets") == Some(&expected_targets);
    if !valid_header {
        return Err("candidate manifest identity does not match promotion inputs".to_owned());
    }
    validate_source_ref(string(&value, "source_ref").unwrap_or_default(), tag)?;
    verify_records(directory, value.get("release_files"), &release_file_names())?;
    verify_records(
        directory,
        value.get("evidence_files"),
        &evidence_file_names(tag)?,
    )?;
    Ok(())
}

fn records(directory: &Path, names: &[String]) -> Result<Vec<Value>, String> {
    names
        .iter()
        .map(|name| {
            let path = directory.join(name);
            Ok(json!({"name": name, "sha256": super::release::checksum(&path)?}))
        })
        .collect()
}

fn verify_records(
    directory: &Path,
    records: Option<&Value>,
    expected: &[String],
) -> Result<(), String> {
    let records = records
        .and_then(Value::as_array)
        .ok_or_else(|| "manifest file records are missing".to_owned())?;
    let actual_names = records
        .iter()
        .filter_map(|record| string(record, "name"))
        .collect::<BTreeSet<_>>();
    let expected_names = expected.iter().map(String::as_str).collect::<BTreeSet<_>>();
    if actual_names != expected_names || records.len() != expected.len() {
        return Err("candidate manifest file set differs from the exact contract".to_owned());
    }
    for record in records {
        let name =
            string(record, "name").ok_or_else(|| "manifest record has no name".to_owned())?;
        let expected_digest = string(record, "sha256")
            .ok_or_else(|| format!("manifest record `{name}` has no digest"))?;
        let actual = super::release::checksum(&directory.join(name))?;
        if actual != expected_digest {
            return Err(format!("candidate file `{name}` digest differs"));
        }
    }
    Ok(())
}

fn reject_unlisted(directory: &Path, release: &[Value], evidence: &[Value]) -> Result<(), String> {
    let expected = release
        .iter()
        .chain(evidence)
        .filter_map(|record| string(record, "name").map(str::to_owned))
        .collect::<BTreeSet<_>>();
    let mut actual = BTreeSet::new();
    collect_files(directory, directory, &mut actual)?;
    (actual == expected).then_some(()).ok_or_else(|| {
        format!(
            "candidate directory contains unlisted files: expected {expected:?}, found {actual:?}"
        )
    })
}

fn collect_files<'a>(
    root: &'a Path,
    directory: &'a Path,
    files: &mut BTreeSet<String>,
) -> Result<(), String> {
    for entry in
        fs::read_dir(directory).map_err(|error| format!("read {}: {error}", directory.display()))?
    {
        let path = entry
            .map_err(|error| format!("read candidate entry: {error}"))?
            .path();
        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else if path.file_name().and_then(|name| name.to_str()) != Some("candidate-manifest.json")
        {
            files.insert(
                path.strip_prefix(root)
                    .map_err(|error| format!("relative candidate path: {error}"))?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

fn release_file_names() -> Vec<String> {
    let mut names = super::release_targets::ALL
        .iter()
        .flat_map(|target| {
            let archive = super::release_targets::archive_name(target);
            [
                archive.clone(),
                format!("{archive}.sha256"),
                format!("{archive}.spdx.json"),
            ]
        })
        .collect::<Vec<_>>();
    names.extend(
        [
            "proqi_amd64.deb",
            "proqi_amd64.deb.sha256",
            "proqi_amd64.deb.spdx.json",
            "SHA256SUMS",
            "proqi.rb",
        ]
        .map(str::to_owned),
    );
    names.sort();
    names
}

fn evidence_file_names(tag: &str) -> Result<Vec<String>, String> {
    let version = tag
        .strip_prefix('v')
        .ok_or_else(|| "candidate tag has no v prefix".to_owned())?;
    Ok(vec![
        "evidence/crate/crate-evidence.json".to_owned(),
        format!("evidence/crate/proqi-{version}.crate"),
        format!("evidence/crate/proqi-{version}.crate.sha256"),
        "evidence/debian/debian-evidence.json".to_owned(),
    ])
}

fn artifact_name(tag: &str, sha: &str) -> String {
    format!("release-candidate-{tag}-{sha}")
}
fn valid_digest(digest: &str) -> bool {
    digest
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}
fn string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}
fn integer(value: &Value, key: &str) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("candidate field `{key}` is missing"))
}
fn parse_u64(raw: &str, label: &str) -> Result<u64, String> {
    raw.parse()
        .map_err(|error| format!("invalid {label} `{raw}`: {error}"))
}
fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn validate_tag_sha(tag: &str, sha: &str) -> Result<(), String> {
    let version = tag
        .strip_prefix('v')
        .ok_or_else(|| "candidate tag must begin with v".to_owned())?;
    let parsed = semver::Version::parse(version)
        .map_err(|error| format!("invalid candidate tag: {error}"))?;
    if tag != format!("v{}.{}.{}", parsed.major, parsed.minor, parsed.patch)
        || !parsed.pre.is_empty()
        || !parsed.build.is_empty()
    {
        return Err("candidate tag must be canonical and stable".to_owned());
    }
    (sha.len() == 40 && sha.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then_some(())
        .ok_or_else(|| "candidate source SHA must be 40 hexadecimal characters".to_owned())
}

fn validate_source_ref(source_ref: &str, tag: &str) -> Result<(), String> {
    (source_ref == "refs/heads/main" || source_ref == format!("refs/tags/{tag}"))
        .then_some(())
        .ok_or_else(|| {
            format!(
                "candidate source ref `{source_ref}` is neither main nor the exact recovery tag"
            )
        })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        create_manifest, evidence_file_names, release_file_names, select_value, verify_manifest,
    };
    use serde_json::json;

    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn index(candidate_count: usize, expired: bool, ci: &str) -> serde_json::Value {
        let candidate = json!({"run_id": 7, "run_attempt": 1, "workflow_path": ".github/workflows/release-candidate.yml", "event": "push", "head_sha": SHA, "source_ref": "refs/heads/main", "conclusion": "success", "artifact": {"id": 9, "name": format!("release-candidate-v1.2.3-{SHA}"), "digest": format!("sha256:{}", "b".repeat(64)), "expired": expired}});
        json!({"schema_version": 1, "required_ci": [{"workflow_path": ".github/workflows/ci.yml", "event": "push", "head_branch": "main", "head_sha": SHA, "conclusion": ci}], "candidates": vec![candidate; candidate_count]})
    }

    #[test]
    fn selection_requires_one_successful_ci_and_one_exact_candidate() {
        assert!(select_value(&index(1, false, "success"), "v1.2.3", SHA).is_ok());
        assert!(select_value(&index(0, false, "success"), "v1.2.3", SHA).is_err());
        assert!(select_value(&index(1, true, "success"), "v1.2.3", SHA).is_err());
        assert!(select_value(&index(2, false, "success"), "v1.2.3", SHA).is_err());
        assert!(select_value(&index(1, false, "failure"), "v1.2.3", SHA).is_err());
        assert!(select_value(&index(1, false, "success"), "v1.2.4", SHA).is_err());
        assert!(
            select_value(
                &index(1, false, "success"),
                "v1.2.3",
                "cccccccccccccccccccccccccccccccccccccccc"
            )
            .is_err()
        );
        let mut failed_candidate = index(1, false, "success");
        failed_candidate["candidates"][0]["conclusion"] = json!("failure");
        assert!(select_value(&failed_candidate, "v1.2.3", SHA).is_err());
    }

    #[test]
    fn manifest_binds_identity_and_rejects_tampered_bytes() {
        let root = tempfile::tempdir().expect("temporary candidate");
        let mut names = release_file_names();
        names.extend(evidence_file_names("v1.2.3").expect("evidence names"));
        for name in &names {
            let path = root.path().join(name);
            fs::create_dir_all(path.parent().expect("candidate parent")).expect("create parent");
            fs::write(path, name).expect("candidate file");
        }
        create_manifest(root.path(), "v1.2.3", "refs/heads/main", SHA, 7, 1).expect("manifest");
        verify_manifest(root.path(), "v1.2.3", SHA, 7, 1).expect("verified manifest");
        fs::write(root.path().join(&names[0]), "tampered").expect("tamper candidate");
        let error = verify_manifest(root.path(), "v1.2.3", SHA, 7, 1).expect_err("tamper rejected");
        assert!(error.contains("digest differs"));
        assert!(verify_manifest(root.path(), "v1.2.3", SHA, 8, 1).is_err());
        assert!(verify_manifest(root.path(), "v1.2.3", SHA, 7, 2).is_err());
    }
}
