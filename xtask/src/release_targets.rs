//! Canonical typed metadata for every supported release target.

use std::{fmt::Write as _, fs, path::Path};

use serde_json::{Value, json};

pub(super) const GLIBC_FLOOR: &str = "2.35";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OperatingSystem {
    MacOs,
    Linux,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Architecture {
    X86_64,
    Arm64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LibcFamily {
    None,
    Gnu,
    Musl,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BuildTool {
    Cargo,
    Zig,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BuildConfiguration {
    runner: &'static str,
    tool: BuildTool,
    docker_platform: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DebianArtifact {
    pub architecture: &'static str,
    pub filename: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ReleaseTarget {
    pub triple: &'static str,
    pub os: OperatingSystem,
    pub architecture: Architecture,
    pub libc: LibcFamily,
    pub runner: &'static str,
    pub build_tool: BuildTool,
    pub docker_platform: Option<&'static str>,
    pub debian: Option<DebianArtifact>,
    pub homebrew: bool,
}

pub(super) const ALL: [ReleaseTarget; 6] = [
    target(
        "aarch64-apple-darwin",
        OperatingSystem::MacOs,
        Architecture::Arm64,
        LibcFamily::None,
        build("macos-15", BuildTool::Cargo, None),
        None,
        true,
    ),
    target(
        "x86_64-apple-darwin",
        OperatingSystem::MacOs,
        Architecture::X86_64,
        LibcFamily::None,
        build("macos-15-intel", BuildTool::Cargo, None),
        None,
        true,
    ),
    target(
        "x86_64-unknown-linux-gnu",
        OperatingSystem::Linux,
        Architecture::X86_64,
        LibcFamily::Gnu,
        build("ubuntu-22.04", BuildTool::Cargo, Some("linux/amd64")),
        Some(DebianArtifact {
            architecture: "amd64",
            filename: "proqi_amd64.deb",
        }),
        true,
    ),
    target(
        "aarch64-unknown-linux-gnu",
        OperatingSystem::Linux,
        Architecture::Arm64,
        LibcFamily::Gnu,
        build("ubuntu-22.04-arm", BuildTool::Cargo, Some("linux/arm64")),
        Some(DebianArtifact {
            architecture: "arm64",
            filename: "proqi_arm64.deb",
        }),
        false,
    ),
    target(
        "x86_64-unknown-linux-musl",
        OperatingSystem::Linux,
        Architecture::X86_64,
        LibcFamily::Musl,
        build("ubuntu-22.04", BuildTool::Zig, Some("linux/amd64")),
        None,
        false,
    ),
    target(
        "aarch64-unknown-linux-musl",
        OperatingSystem::Linux,
        Architecture::Arm64,
        LibcFamily::Musl,
        build("ubuntu-22.04-arm", BuildTool::Zig, Some("linux/arm64")),
        None,
        false,
    ),
];

const fn target(
    triple: &'static str,
    os: OperatingSystem,
    architecture: Architecture,
    libc: LibcFamily,
    build: BuildConfiguration,
    debian: Option<DebianArtifact>,
    homebrew: bool,
) -> ReleaseTarget {
    ReleaseTarget {
        triple,
        os,
        architecture,
        libc,
        runner: build.runner,
        build_tool: build.tool,
        docker_platform: build.docker_platform,
        debian,
        homebrew,
    }
}

const fn build(
    runner: &'static str,
    tool: BuildTool,
    docker_platform: Option<&'static str>,
) -> BuildConfiguration {
    BuildConfiguration {
        runner,
        tool,
        docker_platform,
    }
}

impl ReleaseTarget {
    pub(super) fn archive_name(self) -> String {
        format!("proqi-{}.tar.gz", self.triple)
    }

    pub(super) const fn architecture_name(self) -> &'static str {
        match self.architecture {
            Architecture::X86_64 => "x86-64",
            Architecture::Arm64 => "ARM64",
        }
    }

    pub(super) fn libc_name(self) -> String {
        match self.libc {
            LibcFamily::None => "system".to_owned(),
            LibcFamily::Gnu => format!("glibc >= {GLIBC_FLOOR}"),
            LibcFamily::Musl => "musl/static fallback".to_owned(),
        }
    }

    pub(super) const fn build_command(self) -> &'static str {
        match self.build_tool {
            BuildTool::Cargo => "cargo",
            BuildTool::Zig => "zig",
        }
    }

    pub(super) const fn expected_host(self) -> &'static str {
        match (self.os, self.architecture) {
            (OperatingSystem::MacOs, Architecture::Arm64) => "aarch64-apple-darwin",
            (OperatingSystem::MacOs, Architecture::X86_64) => "x86_64-apple-darwin",
            (OperatingSystem::Linux, Architecture::Arm64) => "aarch64-unknown-linux-gnu",
            (OperatingSystem::Linux, Architecture::X86_64) => "x86_64-unknown-linux-gnu",
        }
    }

    fn matrix_entry(self) -> Value {
        json!({
            "target": self.triple,
            "runner": self.runner,
            "build_tool": self.build_command(),
            "expected_host": self.expected_host(),
            "libc": match self.libc { LibcFamily::None => "none", LibcFamily::Gnu => "gnu", LibcFamily::Musl => "musl" },
            "docker_platform": self.docker_platform,
            "debian_arch": self.debian.map(|package| package.architecture),
            "debian_package": self.debian.map(|package| package.filename),
        })
    }
}

pub(super) fn find(triple: &str) -> Result<ReleaseTarget, String> {
    ALL.into_iter()
        .find(|target| target.triple == triple)
        .ok_or_else(|| format!("unsupported release target `{triple}`"))
}

pub(super) fn triples() -> Vec<&'static str> {
    ALL.iter().map(|target| target.triple).collect()
}

pub(super) fn github_matrix() -> Value {
    Value::Array(ALL.iter().map(|target| target.matrix_entry()).collect())
}

pub(super) fn target_release_files() -> Vec<String> {
    let mut names = ALL
        .iter()
        .flat_map(|target| {
            let archive = target.archive_name();
            [
                archive.clone(),
                format!("{archive}.sha256"),
                format!("{archive}.spdx.json"),
            ]
        })
        .collect::<Vec<_>>();
    for package in ALL.iter().filter_map(|target| target.debian) {
        names.extend([
            package.filename.to_owned(),
            format!("{}.sha256", package.filename),
            format!("{}.spdx.json", package.filename),
        ]);
    }
    names.sort();
    names.dedup();
    names
}

pub(super) fn primary_artifact_names() -> Vec<String> {
    let mut names = ALL
        .iter()
        .map(|target| target.archive_name())
        .collect::<Vec<_>>();
    names.extend(
        ALL.iter()
            .filter_map(|target| target.debian.map(|package| package.filename.to_owned())),
    );
    names.push(super::installer::INSTALLER_NAME.to_owned());
    names.sort();
    names
}

pub(super) fn installer_cases() -> String {
    ALL.iter()
        .map(|target| {
            let os = match target.os {
                OperatingSystem::MacOs => "Darwin",
                OperatingSystem::Linux => "Linux",
            };
            let cpu = match target.architecture {
                Architecture::X86_64 => "x86_64",
                Architecture::Arm64 => "aarch64",
            };
            let libc = match target.libc {
                LibcFamily::None => "none",
                LibcFamily::Gnu => "gnu",
                LibcFamily::Musl => "musl",
            };
            format!("  {os}:{cpu}:{libc}) target='{}' ;;", target.triple)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn print(operation: &str) -> Result<(), String> {
    match operation {
        "github-matrix" => println!("{}", github_matrix()),
        "triples" => ALL.iter().for_each(|target| println!("{}", target.triple)),
        "target-files" => target_release_files()
            .iter()
            .for_each(|name| println!("{name}")),
        "primary-files" => primary_artifact_names()
            .iter()
            .for_each(|name| println!("{name}")),
        "docs" => print!("{}", documentation_table()),
        _ => {
            return Err(
                "release-targets expects github-matrix, triples, target-files, primary-files, or docs"
                    .to_owned(),
            );
        }
    }
    Ok(())
}

pub(super) fn documentation_table() -> String {
    let mut rows = String::from("| OS | CPU | libc | Archive | Debian |\n|---|---|---|---|---|\n");
    for target in ALL {
        let os = match target.os {
            OperatingSystem::MacOs => "macOS",
            OperatingSystem::Linux => "Linux",
        };
        let debian = target.debian.map_or("-", |package| package.filename);
        let _ = writeln!(
            rows,
            "| {os} | {} | {} | `{}` | `{debian}` |",
            target.architecture_name(),
            target.libc_name(),
            target.archive_name()
        );
    }
    rows
}

pub(super) fn policy_findings(root: &Path) -> Result<Vec<String>, String> {
    let readme = fs::read_to_string(root.join("README.md"))
        .map_err(|error| format!("read README.md: {error}"))?;
    let expected = format!(
        "<!-- release-targets:start -->\n{}<!-- release-targets:end -->",
        documentation_table()
    );
    let candidate = fs::read_to_string(root.join(".github/workflows/release-candidate.yml"))
        .map_err(|error| format!("read release candidate workflow: {error}"))?;
    let promotion = fs::read_to_string(root.join(".github/workflows/release.yml"))
        .map_err(|error| format!("read release workflow: {error}"))?;
    let mut findings = Vec::new();
    if !readme.contains(&expected) {
        findings.push("README.md release target table differs from typed metadata".to_owned());
    }
    for required in [
        "cargo xtask release-targets github-matrix",
        "cargo xtask release-targets triples",
        "cargo xtask release-targets primary-files",
        "fromJSON(needs.plan.outputs.target-matrix)",
        "cargo xtask release-files",
    ] {
        if !candidate.contains(required) {
            findings.push(format!(
                "release candidate workflow does not derive `{required}`"
            ));
        }
    }
    if !promotion.contains("cargo xtask release-targets primary-files") {
        findings
            .push("release promotion does not derive primary files from typed metadata".to_owned());
    }
    for target in ALL {
        for (name, source) in [("candidate", &candidate), ("promotion", &promotion)] {
            if source.contains(target.triple) {
                findings.push(format!(
                    "{name} workflow duplicates release target `{}`",
                    target.triple
                ));
            }
        }
    }
    Ok(findings)
}

pub(super) fn validate_debian_evidence(root: &Path, directory: &Path) -> Result<(), String> {
    let directory = if directory.is_absolute() {
        directory.to_path_buf()
    } else {
        root.join(directory)
    };
    for target in ALL {
        let Some(package) = target.debian else {
            continue;
        };
        let path = directory
            .join("evidence/debian")
            .join(package.architecture)
            .join("debian-evidence.json");
        let evidence: Value = serde_json::from_slice(
            &fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?,
        )
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
        let dependency = format!("libc6 (>= {GLIBC_FLOOR})");
        let valid = evidence.get("schema_version").and_then(Value::as_u64) == Some(2)
            && evidence.get("package").and_then(Value::as_str) == Some(package.filename)
            && evidence.get("target").and_then(Value::as_str) == Some(target.triple)
            && evidence
                .get("maintainer_scripts")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            && evidence
                .get("installed_dependencies")
                .and_then(Value::as_array)
                .is_some_and(|dependencies| {
                    dependencies
                        .iter()
                        .any(|value| value.as_str() == Some(dependency.as_str()))
                });
        if !valid {
            return Err(format!(
                "Debian evidence does not match target metadata: {}",
                path.display()
            ));
        }
    }
    println!("verified Debian evidence for every packaged release target");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_unique_and_covers_the_supported_matrix() {
        let triples = triples();
        let unique = triples
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(triples.len(), unique.len());
        assert_eq!(triples.len(), 6);
        assert_eq!(
            ALL.iter().filter(|target| target.debian.is_some()).count(),
            2
        );
        assert!(
            ALL.iter()
                .filter(|target| {
                    target.os == OperatingSystem::Linux
                        && target.architecture == Architecture::Arm64
                })
                .all(|target| target.runner.ends_with("-arm"))
        );
    }

    #[test]
    fn every_primary_artifact_has_checksum_and_sbom_names() {
        let files = target_release_files();
        for target in ALL {
            let archive = target.archive_name();
            assert!(files.contains(&archive));
            assert!(files.contains(&format!("{archive}.sha256")));
            assert!(files.contains(&format!("{archive}.spdx.json")));
        }
        for primary in primary_artifact_names() {
            if primary == crate::installer::INSTALLER_NAME {
                continue;
            }
            assert!(files.contains(&format!("{primary}.sha256")));
            assert!(files.contains(&format!("{primary}.spdx.json")));
        }
    }

    #[test]
    fn workflows_and_documentation_consume_the_registry() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root");
        assert_eq!(policy_findings(root), Ok(Vec::new()));
    }

    #[test]
    fn debian_evidence_validation_uses_exact_target_metadata() {
        let root = tempfile::tempdir().expect("evidence root");
        for target in ALL {
            let Some(package) = target.debian else {
                continue;
            };
            let path = root
                .path()
                .join("evidence/debian")
                .join(package.architecture)
                .join("debian-evidence.json");
            fs::create_dir_all(path.parent().expect("evidence parent")).expect("create parent");
            fs::write(
                path,
                serde_json::to_vec(&json!({
                    "schema_version": 2,
                    "package": package.filename,
                    "target": target.triple,
                    "maintainer_scripts": [],
                    "installed_dependencies": [format!("libc6 (>= {GLIBC_FLOOR})")],
                }))
                .expect("render evidence"),
            )
            .expect("write evidence");
        }
        assert_eq!(validate_debian_evidence(root.path(), root.path()), Ok(()));
        let arm = root
            .path()
            .join("evidence/debian/arm64/debian-evidence.json");
        fs::write(arm, b"{}").expect("tamper evidence");
        assert!(validate_debian_evidence(root.path(), root.path()).is_err());
    }
}
