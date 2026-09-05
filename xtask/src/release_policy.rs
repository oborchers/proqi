//! Executable CI, candidate, publication, and QA-image policy.

use std::{fs, path::Path};
use yaml_rust2::{Yaml, YamlLoader};

const RELEASE_REQUIRED: [&str; 15] = [
    "environment: release",
    "cargo xtask release-promotion-plan",
    "cargo xtask candidate-select",
    "actions/artifacts/${ARTIFACT_ID}/zip",
    "cargo xtask candidate-manifest verify",
    "rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18 # v1.0.5",
    "CARGO_REGISTRY_TOKEN: ${{ steps.crates_auth.outputs.token }}",
    "cargo publish --locked",
    "cargo install proqi --version \"=$VERSION\" --locked",
    "if gh release view \"$TAG\"",
    "cargo xtask release-assets plan",
    "gh release upload",
    "name: Verify every public release byte",
    "name: Wake Homebrew tap synchronization",
    "source-ref \"$SOURCE_REF\"",
];

const CANDIDATE_REQUIRED: [&str; 11] = [
    "branches: [main]",
    "workflow_dispatch:",
    "cargo xtask release-ready",
    "cargo xtask release-promotion-plan",
    "cargo xtask candidate-manifest create",
    "cargo xtask crate-evidence",
    "release-candidate-${{ needs.plan.outputs.tag }}-${{ needs.plan.outputs.source-sha }}",
    "retention-days: 30",
    "artifact-metadata: write",
    "attestations: write",
    "id-token: write",
];

const IMAGE_REQUIRED: [&str; 15] = [
    "ubuntu-24.04-arm",
    "packages: write",
    "github.event_name == 'pull_request'",
    "github.event_name != 'pull_request' && github.ref == 'refs/heads/main'",
    "tools/ci-linux/image.json",
    "tools/ci-linux/**",
    "type=registry,ref=${{ needs.plan.outputs.repository }}:buildcache-",
    "provenance: mode=max",
    "sbom: true",
    "push-to-registry: true",
    "docker logout ghcr.io",
    "workflow_dispatch:",
    "${GITHUB_RUN_ID}",
    "${GITHUB_RUN_ATTEMPT}",
    "Could not prove immutable tag",
];

const HERDR_SENTINEL_PATH: &str = ".github/workflows/herdr-compatibility.yml";
const HERDR_SENTINEL_REQUIRED: [&str; 15] = [
    "schedule:",
    "workflow_dispatch:",
    "if: github.ref == 'refs/heads/main'",
    "permissions: {}",
    "contents: read",
    "issues: write",
    "persist-credentials: false",
    "herdr-linux-x86_64",
    "sha256sum --check --strict",
    "HERDR_CONFIG_PATH=",
    "env -u GH_TOKEN -u GITHUB_TOKEN -u GITHUB_OUTPUT",
    "timeout --kill-after=2s 10s",
    "cargo xtask herdr-compatibility",
    "search/issues",
    "herdr-compatibility:${TAG}:${SCHEMA_SHA256}",
];

pub(crate) fn check(root: &Path) -> Result<Vec<String>, String> {
    let release = read(root, ".github/workflows/release.yml")?;
    let candidate = read(root, ".github/workflows/release-candidate.yml")?;
    let ci = read(root, ".github/workflows/ci.yml")?;
    let image = read(root, ".github/workflows/ci-linux-image.yml")?;
    let sentinel = read(root, HERDR_SENTINEL_PATH)?;
    let mut found = findings(&release, &candidate, &ci, &image);
    found.extend(herdr_sentinel_findings(&sentinel));
    found.extend(image_repository_findings(root)?);
    found.extend(scheduled_workflow_findings(root)?);
    Ok(found)
}

fn read(root: &Path, relative: &str) -> Result<String, String> {
    let path = root.join(relative);
    fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))
}

fn findings(release: &str, candidate: &str, ci: &str, image: &str) -> Vec<String> {
    let mut found = missing(".github/workflows/release.yml", release, &RELEASE_REQUIRED);
    found.extend(missing(
        ".github/workflows/release-candidate.yml",
        candidate,
        &CANDIDATE_REQUIRED,
    ));
    found.extend(missing(
        ".github/workflows/ci-linux-image.yml",
        image,
        &IMAGE_REQUIRED,
    ));
    for forbidden in [
        "CARGO_REGISTRY_TOKEN: ${{ secrets.",
        "uses: ./.github/workflows/release-candidate.yml",
        "cargo dist build",
    ] {
        if release.contains(forbidden) {
            found.push(format!(
                ".github/workflows/release.yml: forbidden `{forbidden}`"
            ));
        }
    }
    for forbidden in [
        "contents: write",
        "packages: write",
        "environment: release",
        "cargo publish",
        "gh release create",
        "cargo xtask crate-package",
    ] {
        if candidate.contains(forbidden) {
            found.push(format!(
                ".github/workflows/release-candidate.yml: publication capability `{forbidden}` is forbidden"
            ));
        }
    }
    for forbidden in ["setup-qemu", ":latest"] {
        if image.contains(forbidden) {
            found.push(format!(
                ".github/workflows/ci-linux-image.yml: forbidden `{forbidden}`"
            ));
        }
    }
    for required in [
        "cargo xtask ci-change-class",
        "name: Registry package contract",
        "cargo xtask crate-package",
        "cargo +1.88.0 xtask msrv-full",
        ".coverage.result == \"skipped\"",
        "name: Required CI result",
    ] {
        if !ci.contains(required) {
            found.push(format!(".github/workflows/ci.yml: missing `{required}`"));
        }
    }
    enforce_order(release, &mut found);
    found
}

fn missing(path: &str, source: &str, markers: &[&str]) -> Vec<String> {
    markers
        .iter()
        .filter(|marker| !source.contains(**marker))
        .map(|marker| format!("{path}: missing `{marker}`"))
        .collect()
}

fn image_repository_findings(root: &Path) -> Result<Vec<String>, String> {
    let repository = ["ghcr.io/oborchers/", "proqi-ci-linux"].concat();
    let mut locations = Vec::new();
    for entry in ignore::WalkBuilder::new(root)
        .standard_filters(true)
        .build()
    {
        let entry = entry.map_err(|error| format!("walk repository: {error}"))?;
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|error| error.to_string())?;
        let contents = fs::read_to_string(entry.path()).unwrap_or_default();
        if contents.contains(&repository) {
            locations.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    if locations == ["tools/ci-linux/image.json"] {
        Ok(Vec::new())
    } else {
        Ok(vec![format!(
            "tools/ci-linux/image.json must exclusively own the public image repository; found {locations:?}"
        )])
    }
}

fn scheduled_workflow_findings(root: &Path) -> Result<Vec<String>, String> {
    let directory = root.join(".github/workflows");
    let mut found = Vec::new();
    for entry in fs::read_dir(&directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?
    {
        let path = entry
            .map_err(|error| format!("read workflow entry: {error}"))?
            .path();
        if !matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("yml" | "yaml")
        ) {
            continue;
        }
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        if schedule_is_forbidden(&path, &source)? {
            found.push(format!(
                "{}: only the Herdr compatibility sentinel may be scheduled",
                path.display()
            ));
        }
    }
    Ok(found)
}

fn schedule_is_forbidden(path: &Path, source: &str) -> Result<bool, String> {
    Ok(contains_schedule(source)?
        && path.file_name().and_then(|name| name.to_str()) != Some("herdr-compatibility.yml"))
}

fn herdr_sentinel_findings(source: &str) -> Vec<String> {
    let mut found = missing(HERDR_SENTINEL_PATH, source, &HERDR_SENTINEL_REQUIRED);
    for forbidden in [
        "contents: write",
        "pull-requests: write",
        "packages: write",
        "id-token: write",
        "persist-credentials: true",
        "test -s \"$schema\"",
    ] {
        if source.contains(forbidden) {
            found.push(format!("{HERDR_SENTINEL_PATH}: forbidden `{forbidden}`"));
        }
    }
    let issue_job = source
        .find("\n  issue:\n")
        .map(|position| &source[position..]);
    if issue_job.is_some_and(|job| job.contains("actions/checkout@")) {
        found.push(format!(
            "{HERDR_SENTINEL_PATH}: the issue job must not check out or execute repository code"
        ));
    }
    found
}

fn contains_schedule(source: &str) -> Result<bool, String> {
    let documents = YamlLoader::load_from_str(source)
        .map_err(|error| format!("parse GitHub Actions workflow YAML: {error}"))?;
    let [document] = documents.as_slice() else {
        return Err("GitHub Actions workflow must contain one YAML document".to_owned());
    };
    let Some(root) = document.as_hash() else {
        return Err("GitHub Actions workflow root must be a mapping".to_owned());
    };
    let trigger = root.get(&Yaml::String("on".to_owned()));
    Ok(trigger.is_some_and(trigger_contains_schedule))
}

fn trigger_contains_schedule(trigger: &Yaml) -> bool {
    match trigger {
        Yaml::String(value) => value == "schedule",
        Yaml::Array(values) => values
            .iter()
            .any(|value| value.as_str() == Some("schedule")),
        Yaml::Hash(values) => values.contains_key(&Yaml::String("schedule".to_owned())),
        _ => false,
    }
}

fn enforce_order(source: &str, found: &mut Vec<String>) {
    const ORDERED: [&str; 8] = [
        "name: Select the exact successful main candidate",
        "name: Verify manifest and every candidate byte",
        "name: Create or verify the immutable release draft",
        "name: Create short-lived crates.io publishing token",
        "name: Publish the verified crate",
        "name: Publish the verified GitHub Release",
        "name: Verify every public release byte",
        "name: Wake Homebrew tap synchronization",
    ];
    let positions = ORDERED
        .iter()
        .filter_map(|marker| source.find(marker))
        .collect::<Vec<_>>();
    if positions.len() == ORDERED.len() && !positions.windows(2).all(|pair| pair[0] < pair[1]) {
        found.push(
            ".github/workflows/release.yml: publication steps violate release ordering".to_owned(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{contains_schedule, findings, herdr_sentinel_findings, schedule_is_forbidden};
    use std::path::Path;

    fn sources() -> (&'static str, &'static str, &'static str, &'static str) {
        (
            include_str!("../../.github/workflows/release.yml"),
            include_str!("../../.github/workflows/release-candidate.yml"),
            include_str!("../../.github/workflows/ci.yml"),
            include_str!("../../.github/workflows/ci-linux-image.yml"),
        )
    }

    #[test]
    fn complete_pipeline_policy_is_accepted() {
        let (release, candidate, ci, image) = sources();
        let found = findings(release, candidate, ci, image);
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn candidate_publication_credentials_are_rejected() {
        let (release, candidate, ci, image) = sources();
        let candidate = format!("{candidate}\ncontents: write\ncargo publish --locked");
        let found = findings(release, &candidate, ci, image);
        assert!(
            found
                .iter()
                .any(|item| item.contains("publication capability"))
        );
    }

    #[test]
    fn rebuild_and_emulation_paths_are_rejected() {
        let (release, candidate, ci, image) = sources();
        let release = format!("{release}\ncargo dist build");
        let image = format!("{image}\nuses: docker/setup-qemu-action@pin");
        let found = findings(&release, candidate, ci, &image);
        assert!(found.iter().any(|item| item.contains("cargo dist build")));
        assert!(found.iter().any(|item| item.contains("setup-qemu")));
    }

    #[test]
    fn docker_trigger_and_publication_idempotency_contracts_are_required() {
        let (release, candidate, ci, image) = sources();
        let release = release.replace("cargo xtask release-assets plan", "false");
        let image = image.replace("tools/ci-linux/**", "tools/elsewhere/**");
        let found = findings(&release, candidate, ci, &image);
        assert!(
            found
                .iter()
                .any(|item| item.contains("release-assets plan"))
        );
        assert!(found.iter().any(|item| item.contains("tools/ci-linux/**")));
        assert!(contains_schedule("on:\n  schedule:\n    - cron: daily").expect("valid YAML"));
        assert!(!contains_schedule("on:\n  workflow_dispatch:").expect("valid YAML"));
    }

    #[test]
    fn product_classifier_and_immutable_image_tags_are_required() {
        let (release, candidate, ci, image) = sources();
        let ci = ci.replace("cargo xtask ci-change-class", "echo");
        let image = image.replace("${GITHUB_RUN_ATTEMPT}", "attempt");
        let found = findings(release, candidate, &ci, &image);
        assert!(found.iter().any(|item| item.contains("ci-change-class")));
        assert!(found.iter().any(|item| item.contains("GITHUB_RUN_ATTEMPT")));
    }

    #[test]
    fn scheduled_herdr_sentinel_is_narrowly_permissioned() {
        let source = include_str!("../../.github/workflows/herdr-compatibility.yml");
        assert!(herdr_sentinel_findings(source).is_empty());
        let unsafe_source = source.replace("contents: read", "contents: write");
        let found = herdr_sentinel_findings(&unsafe_source);
        assert!(found.iter().any(|item| item.contains("contents: read")));
        assert!(found.iter().any(|item| item.contains("contents: write")));
        let checkout_issue = source.replace(
            "steps:\n      - name: Create a deduplicated",
            "steps:\n      - uses: actions/checkout@pin\n      - name: Create a deduplicated",
        );
        assert!(
            herdr_sentinel_findings(&checkout_issue)
                .iter()
                .any(|item| item.contains("must not check out"))
        );
        assert!(
            !schedule_is_forbidden(Path::new("herdr-compatibility.yml"), source)
                .expect("sentinel YAML")
        );
        assert!(
            schedule_is_forbidden(
                Path::new("another.yml"),
                "on:\n  schedule:\n    - cron: daily"
            )
            .expect("scheduled YAML")
        );
        for source in [
            "on:\n  \"schedule\":\n    - cron: daily",
            "on:\n  schedule :\n    - cron: daily",
            "on: {schedule: [{cron: daily}]}",
            "on: [push, schedule]",
        ] {
            assert!(
                schedule_is_forbidden(Path::new("another.yml"), source)
                    .expect("alternate scheduled YAML")
            );
        }
    }
}
