//! Cheap, deterministic release-input and source-identity validation.

use std::{fs, path::Path, process::Command};

use semver::Version;
use serde_json::json;

pub(super) fn print_classification(root: &Path, source_sha: Option<&str>) -> Result<(), String> {
    let version = super::release::workspace_version(root)?;
    let tag = format!("v{version}");
    let source_sha = source_sha.map_or_else(
        || git_text(root, ["rev-parse", "HEAD"]),
        |sha| Ok(sha.to_owned()),
    )?;
    let reasons = readiness_findings(root, &tag, &source_sha, ReleasePhase::Preparation)?;
    println!(
        "{}",
        json!({
            "schema_version": 1,
            "ready": reasons.is_empty(),
            "version": version.to_string(),
            "tag": tag,
            "source_sha": source_sha,
            "reasons": reasons,
        })
    );
    Ok(())
}

pub(super) fn validate_preparation(root: &Path, tag: &str) -> Result<(), String> {
    let source_sha = git_text(root, ["rev-parse", "HEAD"])?;
    require_no_findings(&readiness_findings(
        root,
        tag,
        &source_sha,
        ReleasePhase::Preparation,
    )?)?;
    println!("release inputs are ready: {tag} at {source_sha}");
    Ok(())
}

pub(super) fn validate_promotion(root: &Path, tag: &str) -> Result<(), String> {
    let source_sha = git_text(root, ["rev-parse", "HEAD"])?;
    require_no_findings(&readiness_findings(
        root,
        tag,
        &source_sha,
        ReleasePhase::Promotion,
    )?)?;
    println!("release tag is promotion-ready: {tag} at {source_sha}");
    Ok(())
}

pub(super) fn validate_release_content(root: &Path, tag: &str) -> Result<(), String> {
    let version = super::release::workspace_version(root)?;
    validate_tag(tag, &version)?;
    validate_note(root, tag, &version)?;
    validate_highlights(root, &version)
}

#[derive(Clone, Copy)]
enum ReleasePhase {
    Preparation,
    Promotion,
}

fn readiness_findings(
    root: &Path,
    tag: &str,
    source_sha: &str,
    phase: ReleasePhase,
) -> Result<Vec<String>, String> {
    let mut findings = Vec::new();
    let version = super::release::workspace_version(root)?;
    collect(validate_tag(tag, &version), &mut findings);
    collect(validate_note(root, tag, &version), &mut findings);
    collect(validate_highlights(root, &version), &mut findings);
    collect(validate_source_sha(root, source_sha), &mut findings);
    if matches!(phase, ReleasePhase::Preparation) {
        collect(validate_main_identity(root, source_sha), &mut findings);
    }
    collect(validate_clean_worktree(root), &mut findings);
    collect(
        validate_tag_state(root, tag, source_sha, phase),
        &mut findings,
    );
    Ok(findings)
}

fn collect(result: Result<(), String>, findings: &mut Vec<String>) {
    if let Err(error) = result {
        findings.push(error);
    }
}

fn require_no_findings(findings: &[String]) -> Result<(), String> {
    if findings.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "release inputs are not ready:\n{}",
            findings
                .iter()
                .map(|finding| format!("- {finding}"))
                .collect::<Vec<_>>()
                .join("\n")
        ))
    }
}

fn validate_tag(tag: &str, version: &Version) -> Result<(), String> {
    let expected = format!("v{version}");
    let raw = tag
        .strip_prefix('v')
        .ok_or_else(|| format!("release tag `{tag}` must begin with `v`"))?;
    let parsed = Version::parse(raw).map_err(|error| format!("invalid release tag: {error}"))?;
    let canonical = format!("v{}.{}.{}", parsed.major, parsed.minor, parsed.patch);
    if parsed == *version && parsed.pre.is_empty() && parsed.build.is_empty() && tag == canonical {
        Ok(())
    } else {
        Err(format!(
            "release tag `{tag}` must exactly match stable Cargo version `{expected}`"
        ))
    }
}

fn validate_note(root: &Path, tag: &str, version: &Version) -> Result<(), String> {
    let path = root.join(".github/release-notes").join(format!("{tag}.md"));
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("release notes {} are unavailable: {error}", path.display()))?;
    let expected = format!("# Proqi {version}");
    if contents.lines().next() != Some(expected.as_str()) {
        return Err(format!(
            "release notes {} must begin exactly with `{expected}`",
            path.display()
        ));
    }
    contents
        .lines()
        .skip(1)
        .any(|line| !line.trim().is_empty())
        .then_some(())
        .ok_or_else(|| format!("release notes {} contain no reviewed body", path.display()))
}

fn validate_highlights(root: &Path, version: &Version) -> Result<(), String> {
    super::release_highlights::validate(root, Some(&format!("v{version}")))
}

fn validate_source_sha(root: &Path, expected: &str) -> Result<(), String> {
    let head = git_text(root, ["rev-parse", "HEAD"])?;
    (head == expected)
        .then_some(())
        .ok_or_else(|| format!("source SHA `{expected}` differs from checked-out HEAD `{head}`"))
}

fn validate_main_identity(root: &Path, source_sha: &str) -> Result<(), String> {
    let mut found = Vec::new();
    for reference in ["refs/heads/main", "refs/remotes/origin/main"] {
        if let Some(sha) = optional_git_text(root, ["rev-parse", "--verify", reference])? {
            found.push((reference, sha));
        }
    }
    if found.iter().any(|(_, sha)| sha == source_sha) {
        Ok(())
    } else if found.is_empty() {
        Err("no local main identity is available for release preparation".to_owned())
    } else {
        Err(format!(
            "source SHA is not the exact locally known main commit: {found:?}"
        ))
    }
}

fn validate_clean_worktree(root: &Path) -> Result<(), String> {
    let status = git_text(root, ["status", "--porcelain", "--untracked-files=all"])?;
    status
        .is_empty()
        .then_some(())
        .ok_or_else(|| "release preparation requires a clean Git worktree".to_owned())
}

fn validate_tag_state(
    root: &Path,
    tag: &str,
    source_sha: &str,
    phase: ReleasePhase,
) -> Result<(), String> {
    let reference = format!("refs/tags/{tag}^{{commit}}");
    let existing = optional_git_text(root, ["rev-parse", "--verify", &reference])?;
    match (phase, existing) {
        (ReleasePhase::Preparation, None) => Ok(()),
        (ReleasePhase::Preparation, Some(sha)) => Err(format!(
            "release tag `{tag}` already exists at {sha}; the version is unchanged"
        )),
        (ReleasePhase::Promotion, Some(sha)) if sha == source_sha => Ok(()),
        (ReleasePhase::Promotion, Some(sha)) => Err(format!(
            "release tag `{tag}` points to {sha}, expected {source_sha}"
        )),
        (ReleasePhase::Promotion, None) => Err(format!("release tag `{tag}` does not exist")),
    }
}

fn git_text<I, S>(root: &Path, arguments: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| format!("start git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map(|text| text.trim().to_owned())
        .map_err(|error| format!("git output is not UTF-8: {error}"))
}

fn optional_git_text<I, S>(root: &Path, arguments: I) -> Result<Option<String>, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| format!("start git: {error}"))?;
    if output.status.success() {
        String::from_utf8(output.stdout)
            .map(|text| Some(text.trim().to_owned()))
            .map_err(|error| format!("git output is not UTF-8: {error}"))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, process::Command};

    use super::{ReleasePhase, readiness_findings};

    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("fixture");
        fs::create_dir_all(root.path().join(".github/release-notes")).expect("notes");
        fs::write(
            root.path().join("Cargo.toml"),
            "[workspace]\nmembers=[]\n[workspace.package]\nversion='1.2.3'\n",
        )
        .expect("manifest");
        fs::write(
            root.path().join(".github/release-notes/v1.2.3.md"),
            "# Proqi 1.2.3\n\nReviewed notes.\n",
        )
        .expect("notes");
        fs::write(
            root.path().join("release-highlights.json"),
            r#"{"schema_version":1,"releases":[{"version":"1.2.3","highlights":["One","Two","Three"]}]}"#,
        )
        .expect("highlights");
        git(root.path(), &["init", "-q", "-b", "main"]);
        git(root.path(), &["config", "user.name", "Proqi Test"]);
        git(root.path(), &["config", "user.email", "test@proqi.invalid"]);
        git(root.path(), &["add", "--all"]);
        git(root.path(), &["commit", "-qm", "fixture"]);
        root
    }

    fn git(root: &Path, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(root)
            .output()
            .expect("git");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("UTF-8")
            .trim()
            .to_owned()
    }

    #[test]
    fn release_ready_fixture_rejects_unchanged_version_and_wrong_sha() {
        let root = fixture();
        let sha = git(root.path(), &["rev-parse", "HEAD"]);
        assert!(
            readiness_findings(root.path(), "v1.2.3", &sha, ReleasePhase::Preparation)
                .expect("findings")
                .is_empty()
        );
        assert!(
            readiness_findings(root.path(), "v1.2.3", &sha, ReleasePhase::Preparation)
                .expect("repeated findings")
                .is_empty()
        );
        git(root.path(), &["tag", "v1.2.3"]);
        let unchanged = readiness_findings(root.path(), "v1.2.3", &sha, ReleasePhase::Preparation)
            .expect("findings");
        assert!(
            unchanged
                .iter()
                .any(|finding| finding.contains("unchanged"))
        );
        let wrong = readiness_findings(
            root.path(),
            "v1.2.3",
            &"f".repeat(40),
            ReleasePhase::Preparation,
        )
        .expect("findings");
        assert!(wrong.iter().any(|finding| finding.contains("source SHA")));
    }

    #[test]
    fn missing_or_malformed_review_inputs_fail_closed() {
        let root = fixture();
        let sha = git(root.path(), &["rev-parse", "HEAD"]);
        fs::remove_file(root.path().join(".github/release-notes/v1.2.3.md")).expect("remove notes");
        let missing = readiness_findings(root.path(), "v1.2.3", &sha, ReleasePhase::Preparation)
            .expect("findings");
        assert!(
            missing
                .iter()
                .any(|finding| finding.contains("release notes") && finding.contains("unavailable"))
        );

        let root = fixture();
        let sha = git(root.path(), &["rev-parse", "HEAD"]);
        fs::write(root.path().join("release-highlights.json"), "not json").expect("tamper");
        let findings = readiness_findings(root.path(), "v1.2.3", &sha, ReleasePhase::Preparation)
            .expect("findings");
        assert!(
            findings
                .iter()
                .any(|finding| finding.contains("parse release highlights"))
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.contains("clean Git worktree"))
        );
    }

    #[test]
    fn promotion_remains_valid_after_main_advances() {
        let root = fixture();
        let release_sha = git(root.path(), &["rev-parse", "HEAD"]);
        git(root.path(), &["tag", "v1.2.3"]);
        fs::write(root.path().join("later.txt"), "main advanced\n").expect("later change");
        git(root.path(), &["add", "later.txt"]);
        git(root.path(), &["commit", "-qm", "later main change"]);
        git(root.path(), &["checkout", "-q", "--detach", &release_sha]);

        let findings =
            readiness_findings(root.path(), "v1.2.3", &release_sha, ReleasePhase::Promotion)
                .expect("promotion findings");
        assert!(findings.is_empty(), "{findings:#?}");
    }
}
