//! Repository-owned CI change classification.

use std::{ffi::OsStr, path::Path, process::Command};

use serde_json::json;

#[derive(Debug, PartialEq, Eq)]
struct Classification {
    docs_only: bool,
    coverage: bool,
    full_msrv: bool,
}

pub(super) fn print(root: &Path, base_sha: &str, head_sha: &str) -> Result<(), String> {
    let paths = changed_paths(root, base_sha, head_sha)?;
    let classification = classify(paths.iter().map(String::as_str));
    println!(
        "{}",
        json!({
            "schema_version": 1,
            "base_sha": base_sha,
            "head_sha": head_sha,
            "docs_only": classification.docs_only,
            "coverage": classification.coverage,
            "full_msrv": classification.full_msrv,
        })
    );
    Ok(())
}

fn changed_paths(root: &Path, base_sha: &str, head_sha: &str) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .args([
            "diff",
            "--name-only",
            "--diff-filter=ACDMRTUXB",
            base_sha,
            head_sha,
            "--",
        ])
        .current_dir(root)
        .output()
        .map_err(|error| format!("start git diff: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git diff exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("git diff output is not UTF-8: {error}"))
        .map(|text| text.lines().map(str::to_owned).collect())
}

fn classify<'a>(paths: impl Iterator<Item = &'a str>) -> Classification {
    let paths = paths.collect::<Vec<_>>();
    Classification {
        docs_only: !paths.is_empty() && paths.iter().all(|path| is_ordinary_markdown(path)),
        coverage: paths.iter().any(|path| requires_coverage(path)),
        full_msrv: paths.iter().any(|path| requires_full_msrv(path)),
    }
}

fn is_ordinary_markdown(path: &str) -> bool {
    has_extension(path, "md") && !path.starts_with(".github/release-notes/")
}

fn requires_coverage(path: &str) -> bool {
    has_extension(path, "rs")
        || matches!(
            path,
            "Cargo.toml" | "Cargo.lock" | "rust-toolchain.toml" | "rust-toolchain"
        )
}

fn requires_full_msrv(path: &str) -> bool {
    matches!(
        path,
        "Cargo.toml" | "Cargo.lock" | "rust-toolchain.toml" | "rust-toolchain"
    ) || path.ends_with("/Cargo.toml")
        || path.starts_with(".cargo/")
        || matches!(path, "xtask/src/package.rs" | "xtask/src/crate_package.rs")
        || path.strip_prefix("xtask/src/").is_some_and(|name| {
            (name.starts_with("debian") || name.starts_with("release")) && has_extension(name, "rs")
        })
        || path.starts_with(".github/workflows/")
        || path.starts_with(".github/actions/")
        || path.starts_with("tools/ci-linux/")
}

fn has_extension(path: &str, extension: &str) -> bool {
    Path::new(path).extension() == Some(OsStr::new(extension))
}

#[cfg(test)]
mod tests {
    use super::{Classification, classify};

    fn paths(values: &[&str]) -> Classification {
        classify(values.iter().copied())
    }

    #[test]
    fn ordinary_markdown_remains_docs_only() {
        assert_eq!(
            paths(&["README.md", "docs/usage.md"]),
            Classification {
                docs_only: true,
                coverage: false,
                full_msrv: false,
            }
        );
    }

    #[test]
    fn reviewed_release_inputs_require_product_ci() {
        assert!(!paths(&[".github/release-notes/v1.2.3.md"]).docs_only);
        assert!(!paths(&["release-highlights.json"]).docs_only);
    }

    #[test]
    fn source_and_packaging_boundaries_keep_existing_gates() {
        let source = paths(&["src/main.rs"]);
        assert!(source.coverage);
        assert!(!source.full_msrv);

        let packaging = paths(&["xtask/src/debian_verify.rs"]);
        assert!(packaging.coverage);
        assert!(packaging.full_msrv);
    }

    #[test]
    fn empty_diff_is_not_docs_only() {
        assert_eq!(
            paths(&[]),
            Classification {
                docs_only: false,
                coverage: false,
                full_msrv: false,
            }
        );
    }
}
