//! Executable release-workflow policy.

use std::{fs, path::Path};

const RELEASE_REQUIRED: [&str; 13] = [
    "uses: ./.github/workflows/release-candidate.yml",
    "needs: candidate",
    "environment: release",
    "id-token: write",
    "name: release-candidate-${{ github.ref_name }}",
    ".schema_version == 3",
    "rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18 # v1.0.5",
    "CARGO_REGISTRY_TOKEN: ${{ steps.crates_auth.outputs.token }}",
    "cargo publish --locked",
    "cargo install proqi --version \"=$VERSION\" --locked",
    "https://crates.io/api/v1/crates/proqi/${VERSION}/download",
    "name: Verify every public release byte",
    "name: Wake Homebrew tap synchronization",
];

const CANDIDATE_REQUIRED: [&str; 5] = [
    "workflow_call:",
    "workflow_dispatch:",
    "source-ref=$GITHUB_REF",
    "schema_version: 3",
    "source_ref: $source_ref",
];

pub(crate) fn check(root: &Path) -> Result<Vec<String>, String> {
    let release_path = root.join(".github/workflows/release.yml");
    let release = fs::read_to_string(&release_path)
        .map_err(|error| format!("read {}: {error}", release_path.display()))?;
    let candidate_path = root.join(".github/workflows/release-candidate.yml");
    let candidate = fs::read_to_string(&candidate_path)
        .map_err(|error| format!("read {}: {error}", candidate_path.display()))?;
    Ok(findings(&release, &candidate))
}

fn findings(release: &str, candidate: &str) -> Vec<String> {
    let mut findings = RELEASE_REQUIRED
        .iter()
        .filter(|marker| !release.contains(**marker))
        .map(|marker| format!(".github/workflows/release.yml: missing `{marker}`"))
        .collect::<Vec<_>>();
    findings.extend(
        CANDIDATE_REQUIRED
            .iter()
            .filter(|marker| !candidate.contains(**marker))
            .map(|marker| format!(".github/workflows/release-candidate.yml: missing `{marker}`")),
    );
    if release.contains("CARGO_REGISTRY_TOKEN: ${{ secrets.") {
        findings.push(
            ".github/workflows/release.yml: long-lived registry secret is forbidden".to_owned(),
        );
    }
    enforce_order(release, &mut findings);
    findings
}

fn enforce_order(source: &str, findings: &mut Vec<String>) {
    const ORDERED: [&str; 6] = [
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
        findings.push(
            ".github/workflows/release.yml: publication steps violate release ordering".to_owned(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::findings;

    #[test]
    fn trusted_registry_workflow_is_accepted() {
        let release = include_str!("../../.github/workflows/release.yml");
        let candidate = include_str!("../../.github/workflows/release-candidate.yml");
        assert!(findings(release, candidate).is_empty());
    }

    #[test]
    fn registry_secret_and_inverted_release_order_are_rejected() {
        let release = include_str!("../../.github/workflows/release.yml")
            .replace(
                "CARGO_REGISTRY_TOKEN: ${{ steps.crates_auth.outputs.token }}",
                "CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}",
            )
            .replace(
                "name: Publish the verified GitHub Release",
                "name: Publish the verified GitHub Release early",
            )
            .replace(
                "name: Create or verify the immutable release draft",
                "name: Publish the verified GitHub Release",
            )
            .replace(
                "name: Publish the verified GitHub Release early",
                "name: Create or verify the immutable release draft",
            );
        let candidate = include_str!("../../.github/workflows/release-candidate.yml");
        let findings = findings(&release, candidate);
        assert!(findings.iter().any(|finding| finding.contains("secret")));
        assert!(findings.iter().any(|finding| finding.contains("ordering")));
    }

    #[test]
    fn detached_manual_candidate_contract_is_rejected() {
        let release = include_str!("../../.github/workflows/release.yml").replace(
            "uses: ./.github/workflows/release-candidate.yml",
            "uses: missing.yml",
        );
        let candidate = include_str!("../../.github/workflows/release-candidate.yml")
            .replace("workflow_call:", "workflow_call_removed:");
        let findings = findings(&release, &candidate);
        assert!(
            findings
                .iter()
                .any(|finding| finding.contains("release.yml"))
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.contains("release-candidate.yml"))
        );
    }

    #[test]
    fn homebrew_cannot_run_before_public_bytes_are_verified() {
        let release = include_str!("../../.github/workflows/release.yml")
            .replace(
                "name: Verify every public release byte",
                "name: Tap wake early",
            )
            .replace(
                "name: Wake Homebrew tap synchronization",
                "name: Verify every public release byte",
            )
            .replace(
                "name: Tap wake early",
                "name: Wake Homebrew tap synchronization",
            );
        let candidate = include_str!("../../.github/workflows/release-candidate.yml");
        let findings = findings(&release, candidate);
        assert!(findings.iter().any(|finding| finding.contains("ordering")));
    }
}
