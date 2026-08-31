//! Idempotent reconciliation of verified candidate bytes with a GitHub Release.

use std::{collections::BTreeSet, fs, path::Path};

use serde_json::{Value, json};

pub(super) fn plan(
    root: &Path,
    candidate: &Path,
    existing: &Path,
    release_state: &Path,
) -> Result<(), String> {
    let candidate = resolve(root, candidate);
    let existing = resolve(root, existing);
    let release_state = resolve(root, release_state);
    let (is_draft, missing) = reconciliation(&candidate, &existing, &release_state)?;
    println!(
        "{}",
        json!({"schema_version": 1, "is_draft": is_draft, "missing": missing})
    );
    Ok(())
}

fn reconciliation(
    candidate: &Path,
    existing: &Path,
    release_state: &Path,
) -> Result<(bool, Vec<String>), String> {
    let expected = expected_assets(candidate)?;
    let (is_draft, published) = read_release_state(release_state)?;
    let downloaded = directory_files(existing)?;

    if published != downloaded {
        return Err(format!(
            "release metadata and downloaded assets differ: metadata {published:?}, downloaded {downloaded:?}"
        ));
    }
    let unexpected = published.difference(&expected).collect::<Vec<_>>();
    if !unexpected.is_empty() {
        return Err(format!(
            "release contains unexpected assets: {unexpected:?}"
        ));
    }
    for name in &published {
        let candidate_digest = super::release::checksum(&candidate.join(name))?;
        let published_digest = super::release::checksum(&existing.join(name))?;
        if candidate_digest != published_digest {
            return Err(format!(
                "existing release asset `{name}` differs from candidate"
            ));
        }
    }

    let missing = expected.difference(&published).cloned().collect::<Vec<_>>();
    if !is_draft && !missing.is_empty() {
        return Err(format!(
            "published release is incomplete; missing candidate assets: {missing:?}"
        ));
    }
    Ok((is_draft, missing))
}

fn resolve(root: &Path, path: &Path) -> std::path::PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}

fn expected_assets(candidate: &Path) -> Result<BTreeSet<String>, String> {
    let path = candidate.join("candidate-manifest.json");
    let manifest: Value = serde_json::from_slice(
        &fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?,
    )
    .map_err(|error| format!("parse candidate manifest: {error}"))?;
    let records = manifest
        .get("release_files")
        .and_then(Value::as_array)
        .ok_or_else(|| "candidate manifest has no release_files array".to_owned())?;
    let mut names = BTreeSet::new();
    for record in records {
        let name = record
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "candidate release file has no name".to_owned())?;
        validate_asset_name(name)?;
        let digest = record
            .get("sha256")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("candidate release file `{name}` has no digest"))?;
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!(
                "candidate release file `{name}` has an invalid digest"
            ));
        }
        if !names.insert(name.to_owned()) {
            return Err(format!("candidate manifest repeats release file `{name}`"));
        }
        let actual = super::release::checksum(&candidate.join(name))?;
        if actual != digest {
            return Err(format!(
                "candidate release file `{name}` differs from its manifest"
            ));
        }
    }
    if names.is_empty() {
        return Err("candidate manifest has no release files".to_owned());
    }
    Ok(names)
}

fn validate_asset_name(name: &str) -> Result<(), String> {
    let path = Path::new(name);
    (path.file_name().and_then(|value| value.to_str()) == Some(name) && name != ".")
        .then_some(())
        .ok_or_else(|| format!("release asset name `{name}` is not a plain file name"))
}

fn read_release_state(path: &Path) -> Result<(bool, BTreeSet<String>), String> {
    let state: Value = serde_json::from_slice(
        &fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?,
    )
    .map_err(|error| format!("parse release state: {error}"))?;
    if state.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err("release state requires schema_version 1".to_owned());
    }
    let is_draft = state
        .get("is_draft")
        .and_then(Value::as_bool)
        .ok_or_else(|| "release state has no is_draft boolean".to_owned())?;
    let assets = state
        .get("assets")
        .and_then(Value::as_array)
        .ok_or_else(|| "release state has no assets array".to_owned())?;
    let mut names = BTreeSet::new();
    for asset in assets {
        let name = asset
            .as_str()
            .ok_or_else(|| "release state asset names must be strings".to_owned())?;
        validate_asset_name(name)?;
        if !names.insert(name.to_owned()) {
            return Err(format!("release metadata repeats asset `{name}`"));
        }
    }
    Ok((is_draft, names))
}

fn directory_files(directory: &Path) -> Result<BTreeSet<String>, String> {
    let mut names = BTreeSet::new();
    for entry in
        fs::read_dir(directory).map_err(|error| format!("read {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("read release asset entry: {error}"))?;
        if !entry
            .file_type()
            .map_err(|error| format!("read release asset type: {error}"))?
            .is_file()
        {
            return Err(format!(
                "downloaded release entry {} is not a regular file",
                entry.path().display()
            ));
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "downloaded release asset name is not UTF-8".to_owned())?;
        names.insert(name);
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use serde_json::json;

    use super::{expected_assets, read_release_state, reconciliation};

    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("fixture");
        fs::create_dir(root.path().join("candidate")).expect("candidate");
        fs::create_dir(root.path().join("existing")).expect("existing");
        for (name, contents) in [("one.tar.gz", "one"), ("two.deb", "two")] {
            fs::write(root.path().join("candidate").join(name), contents).expect("asset");
        }
        let records = ["one.tar.gz", "two.deb"].map(|name| {
            json!({
                "name": name,
                "sha256": crate::release::checksum(&root.path().join("candidate").join(name))
                    .expect("digest")
            })
        });
        fs::write(
            root.path().join("candidate/candidate-manifest.json"),
            serde_json::to_vec(&json!({"release_files": records})).expect("manifest"),
        )
        .expect("manifest");
        root
    }

    fn state(root: &Path, draft: bool, assets: &[&str]) {
        fs::write(
            root.join("state.json"),
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "is_draft": draft,
                "assets": assets,
            }))
            .expect("state"),
        )
        .expect("state");
    }

    fn reconcile(root: &Path) -> Result<(bool, Vec<String>), String> {
        reconciliation(
            &root.join("candidate"),
            &root.join("existing"),
            &root.join("state.json"),
        )
    }

    #[test]
    fn empty_and_partial_drafts_are_resumable() {
        let root = fixture();
        state(root.path(), true, &[]);
        assert_eq!(
            reconcile(root.path()).expect("empty draft"),
            (true, vec!["one.tar.gz".to_owned(), "two.deb".to_owned()])
        );

        fs::write(root.path().join("existing/one.tar.gz"), "one").expect("existing");
        state(root.path(), true, &["one.tar.gz"]);
        assert_eq!(
            reconcile(root.path()).expect("partial draft"),
            (true, vec!["two.deb".to_owned()])
        );
    }

    #[test]
    fn complete_draft_and_public_release_are_idempotent() {
        for draft in [true, false] {
            let root = fixture();
            fs::copy(
                root.path().join("candidate/one.tar.gz"),
                root.path().join("existing/one.tar.gz"),
            )
            .expect("copy one");
            fs::copy(
                root.path().join("candidate/two.deb"),
                root.path().join("existing/two.deb"),
            )
            .expect("copy two");
            state(root.path(), draft, &["one.tar.gz", "two.deb"]);
            assert_eq!(
                reconcile(root.path()).expect("complete release"),
                (draft, Vec::new())
            );
        }
    }

    #[test]
    fn conflicts_unexpected_assets_and_incomplete_public_releases_fail_closed() {
        let root = fixture();
        fs::write(root.path().join("existing/one.tar.gz"), "tampered").expect("tamper");
        state(root.path(), true, &["one.tar.gz"]);
        assert!(
            reconcile(root.path())
                .expect_err("conflict")
                .contains("differs")
        );

        let root = fixture();
        fs::write(root.path().join("existing/surprise.zip"), "surprise").expect("unexpected");
        state(root.path(), true, &["surprise.zip"]);
        assert!(
            reconcile(root.path())
                .expect_err("unexpected")
                .contains("unexpected")
        );

        let root = fixture();
        state(root.path(), false, &[]);
        assert!(
            reconcile(root.path())
                .expect_err("incomplete public")
                .contains("incomplete")
        );
    }

    #[test]
    fn duplicate_metadata_and_manifest_entries_are_rejected() {
        let root = fixture();
        state(root.path(), true, &["one.tar.gz", "one.tar.gz"]);
        assert!(
            read_release_state(&root.path().join("state.json"))
                .expect_err("duplicate metadata")
                .contains("repeats")
        );

        let manifest = json!({"release_files": [
            {"name":"one.tar.gz", "sha256": crate::release::checksum(&root.path().join("candidate/one.tar.gz")).expect("digest")},
            {"name":"one.tar.gz", "sha256": crate::release::checksum(&root.path().join("candidate/one.tar.gz")).expect("digest")}
        ]});
        fs::write(
            root.path().join("candidate/candidate-manifest.json"),
            serde_json::to_vec(&manifest).expect("manifest"),
        )
        .expect("manifest");
        assert!(
            expected_assets(&root.path().join("candidate"))
                .expect_err("duplicate manifest")
                .contains("repeats")
        );
    }
}
