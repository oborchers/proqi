//! Shared release-highlight and GitHub-note agreement gate.

use std::{collections::BTreeSet, fs, path::Path};

use proqi::domain::{RELEASE_HIGHLIGHTS_MAX_BYTES, ReleaseHighlightsManifest, StableVersion};

pub(super) fn validate(root: &Path, requested_tag: Option<&str>) -> Result<(), String> {
    let path = root.join("release-highlights.json");
    let metadata = fs::metadata(&path).map_err(|error| {
        format!(
            "release highlights {} are unavailable: {error}",
            path.display()
        )
    })?;
    if !metadata.is_file()
        || metadata.len() > u64::try_from(RELEASE_HIGHLIGHTS_MAX_BYTES).unwrap_or(u64::MAX)
    {
        return Err(format!(
            "release highlights {} must be a bounded regular file",
            path.display()
        ));
    }
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("read release highlights {}: {error}", path.display()))?;
    let manifest = ReleaseHighlightsManifest::parse_json(&contents)
        .map_err(|error| format!("validate release highlights: {error}"))?;
    let current = StableVersion::parse(&super::release::workspace_version(root)?.to_string())
        .map_err(|error| format!("validate workspace version: {error}"))?;
    validate_agreement(
        &manifest,
        &root.join(".github/release-notes"),
        &current,
        requested_tag,
    )
}

fn validate_agreement(
    manifest: &ReleaseHighlightsManifest,
    notes: &Path,
    current: &StableVersion,
    requested_tag: Option<&str>,
) -> Result<(), String> {
    let represented = manifest
        .releases()
        .iter()
        .map(|release| release.version().clone())
        .collect::<BTreeSet<_>>();
    if represented.last() != Some(current) {
        return Err(format!(
            "latest release highlights must exactly match Cargo version {current}"
        ));
    }
    if let Some(tag) = requested_tag {
        let requested = StableVersion::parse_tag(tag)
            .map_err(|_| format!("release highlights tag `{tag}` is not canonical"))?;
        if &requested != current || !represented.contains(&requested) {
            return Err(format!(
                "release highlights do not exactly represent requested tag `{tag}`"
            ));
        }
    }
    let reviewed_versions = note_versions(notes)?;
    if represented != reviewed_versions {
        return Err(format!(
            "release highlight versions differ from reviewed GitHub notes: highlights {represented:?}, notes {reviewed_versions:?}"
        ));
    }
    Ok(())
}

fn note_versions(directory: &Path) -> Result<BTreeSet<StableVersion>, String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("read release notes {}: {error}", directory.display()))?;
    let mut versions = BTreeSet::new();
    for entry in entries {
        let path = entry
            .map_err(|error| format!("read release note entry: {error}"))?
            .path();
        if path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }
        let version = note_version(&path)?;
        if !versions.insert(version.clone()) {
            return Err(format!("duplicate release note version {version}"));
        }
    }
    Ok(versions)
}

fn note_version(path: &Path) -> Result<StableVersion, String> {
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("release note path is not UTF-8: {}", path.display()))?;
    let tag = filename
        .strip_suffix(".md")
        .ok_or_else(|| format!("release note filename is invalid: {filename}"))?;
    let version = StableVersion::parse_tag(tag)
        .map_err(|_| format!("release note filename is not canonical: {filename}"))?;
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("read release note {}: {error}", path.display()))?;
    let expected_title = format!("# Proqi {version}");
    if contents.lines().next() != Some(expected_title.as_str()) {
        return Err(format!(
            "release note {} must begin exactly with `{expected_title}`",
            path.display()
        ));
    }
    if contents.lines().skip(1).all(|line| line.trim().is_empty()) {
        return Err(format!(
            "release note {} must contain reviewed notes after its title",
            path.display()
        ));
    }
    Ok(version)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use proqi::domain::{ReleaseHighlightsManifest, StableVersion};

    use super::validate_agreement;

    fn manifest() -> ReleaseHighlightsManifest {
        ReleaseHighlightsManifest::parse_json(
            r#"{"schema_version":1,"releases":[{"version":"1.0.0","highlights":["One","Two","Three"]}]}"#,
        )
        .expect("manifest")
    }

    #[test]
    fn exact_note_title_version_and_requested_tag_agree() {
        let temporary = tempfile::tempdir().expect("temporary notes");
        fs::write(
            temporary.path().join("v1.0.0.md"),
            "# Proqi 1.0.0\n\nReviewed notes.\n",
        )
        .expect("note");
        let current = StableVersion::parse("1.0.0").expect("current");
        assert!(
            validate_agreement(&manifest(), temporary.path(), &current, Some("v1.0.0")).is_ok()
        );
        assert!(
            validate_agreement(&manifest(), temporary.path(), &current, Some("v1.0.1")).is_err()
        );
    }

    #[test]
    fn missing_extra_and_mistitled_notes_fail() {
        let temporary = tempfile::tempdir().expect("temporary notes");
        let current = StableVersion::parse("1.0.0").expect("current");
        assert!(validate_agreement(&manifest(), temporary.path(), &current, None).is_err());
        fs::write(
            temporary.path().join("v1.0.0.md"),
            "# Proqi v1.0.0\n\nNotes.\n",
        )
        .expect("mistitled note");
        assert!(validate_agreement(&manifest(), temporary.path(), &current, None).is_err());
        fs::write(
            temporary.path().join("v1.0.0.md"),
            "# Proqi 1.0.0\n\nNotes.\n",
        )
        .expect("note");
        fs::write(
            temporary.path().join("v1.1.0.md"),
            "# Proqi 1.1.0\n\nExtra.\n",
        )
        .expect("extra note");
        assert!(validate_agreement(&manifest(), temporary.path(), &current, None).is_err());
    }
}
