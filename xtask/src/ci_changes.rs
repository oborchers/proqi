//! Repository-owned CI and local-gate change classification.

use std::{
    collections::BTreeSet,
    ffi::OsStr,
    path::Path,
    process::{Command, Output},
};

use serde_json::json;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum ChangeClass {
    CiPolicy,
    Dependencies,
    Documentation,
    Packaging,
    PersistenceContract,
    Product,
    Release,
    Terminal,
    Tooling,
    Unknown,
}

impl ChangeClass {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::CiPolicy => "ci_policy",
            Self::Dependencies => "dependencies",
            Self::Documentation => "documentation",
            Self::Packaging => "packaging",
            Self::PersistenceContract => "persistence_contract",
            Self::Product => "product",
            Self::Release => "release",
            Self::Terminal => "terminal",
            Self::Tooling => "tooling",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LocalPlan {
    Documentation,
    Fast,
    Full,
}

impl LocalPlan {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Documentation => "documentation",
            Self::Fast => "fast",
            Self::Full => "full",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Classification {
    pub(super) docs_only: bool,
    pub(super) coverage: bool,
    pub(super) full_msrv: bool,
    pub(super) classes: BTreeSet<ChangeClass>,
    pub(super) local_plan: LocalPlan,
}

pub(super) struct LocalChanges {
    pub(super) base: String,
    pub(super) head: String,
    pub(super) paths: Vec<String>,
    pub(super) untracked: BTreeSet<String>,
    pub(super) classification: Classification,
}

pub(super) fn print(root: &Path, base_sha: &str, head_sha: &str) -> Result<(), String> {
    let paths = changed_paths(root, base_sha, Some(head_sha))?;
    let classification = classify(paths.iter().map(String::as_str));
    println!(
        "{}",
        classification_json(&classification, base_sha, head_sha)
    );
    Ok(())
}

pub(super) fn local(root: &Path, comparison: Option<&str>) -> Result<LocalChanges, String> {
    let comparison = comparison.unwrap_or("origin/main");
    let comparison = resolve_commit(root, comparison)?;
    let head = resolve_commit(root, "HEAD")?;
    let base = command_text(root, ["merge-base", head.as_str(), comparison.as_str()])?
        .trim()
        .to_owned();
    if base.is_empty() {
        return Err("git merge-base returned no commit".to_owned());
    }
    let mut paths = changed_paths(root, &base, None)?;
    let untracked = untracked_paths(root)?.into_iter().collect::<BTreeSet<_>>();
    paths.extend(untracked.iter().cloned());
    paths.sort();
    paths.dedup();
    let classification = classify(paths.iter().map(String::as_str));
    Ok(LocalChanges {
        base,
        head,
        paths,
        untracked,
        classification,
    })
}

pub(super) fn classification_json(
    classification: &Classification,
    base_sha: &str,
    head_sha: &str,
) -> serde_json::Value {
    let classes = classification
        .classes
        .iter()
        .map(|class| class.name())
        .collect::<Vec<_>>();
    let high_risk = classification.local_plan == LocalPlan::Full;
    let terminal = classification.classes.contains(&ChangeClass::Terminal) || high_risk;
    let packaging = classification.classes.contains(&ChangeClass::Packaging)
        || classification.classes.contains(&ChangeClass::Release)
        || classification.classes.contains(&ChangeClass::Dependencies)
        || high_risk;
    json!({
        "schema_version": 2,
        "base_sha": base_sha,
        "head_sha": head_sha,
        "docs_only": classification.docs_only,
        "coverage": classification.coverage,
        "full_msrv": classification.full_msrv,
        "classes": classes,
        "local_plan": classification.local_plan.name(),
        "advisory": {
            "pty": advisory(terminal),
            "security": advisory(
                classification.classes.contains(&ChangeClass::Dependencies) || high_risk
            ),
            "registry_package": advisory(packaging),
            "platform_package": advisory(packaging),
            "debian": advisory(packaging),
        },
    })
}

fn advisory(required: bool) -> &'static str {
    if required {
        "required"
    } else {
        "candidate_skip"
    }
}

fn changed_paths(
    root: &Path,
    base_sha: &str,
    head_sha: Option<&str>,
) -> Result<Vec<String>, String> {
    let mut arguments = vec![
        "diff",
        "--name-status",
        "-z",
        "--find-renames",
        "--diff-filter=ACDMRTUXB",
        base_sha,
    ];
    if let Some(head_sha) = head_sha {
        arguments.push(head_sha);
    }
    arguments.push("--");
    parse_name_status(command(root, arguments)?, "git diff")
}

fn untracked_paths(root: &Path) -> Result<Vec<String>, String> {
    parse_nul_paths(
        &checked_stdout(
            command(root, ["ls-files", "-z", "--others", "--exclude-standard"])?,
            "git ls-files",
        )?,
        "git ls-files",
    )
}

fn checked_stdout(output: Output, operation: &str) -> Result<Vec<u8>, String> {
    if !output.status.success() {
        return Err(format!(
            "{operation} exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

fn parse_name_status(output: Output, operation: &str) -> Result<Vec<String>, String> {
    let output = checked_stdout(output, operation)?;
    let mut fields = output
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty());
    let mut paths = Vec::new();
    while let Some(status) = fields.next() {
        let status = std::str::from_utf8(status)
            .map_err(|error| format!("{operation} status is not UTF-8: {error}"))?;
        let path_count = if status.starts_with('R') || status.starts_with('C') {
            2
        } else {
            1
        };
        for _ in 0..path_count {
            let path = fields
                .next()
                .ok_or_else(|| format!("{operation} returned incomplete status `{status}`"))?;
            paths.push(
                std::str::from_utf8(path)
                    .map_err(|error| format!("{operation} path is not UTF-8: {error}"))?
                    .to_owned(),
            );
        }
    }
    Ok(paths)
}

fn parse_nul_paths(output: &[u8], operation: &str) -> Result<Vec<String>, String> {
    output
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .map(|path| {
            std::str::from_utf8(path)
                .map(str::to_owned)
                .map_err(|error| format!("{operation} path is not UTF-8: {error}"))
        })
        .collect()
}

fn resolve_commit(root: &Path, revision: &str) -> Result<String, String> {
    command_text(
        root,
        ["rev-parse", "--verify", &format!("{revision}^{{commit}}")],
    )
    .map(|value| value.trim().to_owned())
}

fn command_text<const N: usize>(root: &Path, arguments: [&str; N]) -> Result<String, String> {
    let output = command(root, arguments)?;
    if !output.status.success() {
        return Err(format!(
            "git {} exited with {}: {}",
            arguments.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| format!("git output is not UTF-8: {error}"))
}

fn command<I, S>(root: &Path, arguments: I) -> Result<Output, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| format!("start git: {error}"))
}

fn classify<'a>(paths: impl Iterator<Item = &'a str>) -> Classification {
    let paths = paths.collect::<Vec<_>>();
    let mut classes = BTreeSet::new();
    for path in &paths {
        classify_path(path, &mut classes);
    }
    if paths.is_empty() {
        classes.insert(ChangeClass::Unknown);
    }
    let docs_only = !paths.is_empty() && paths.iter().all(|path| is_ordinary_markdown(path));
    let coverage = paths.iter().any(|path| requires_coverage(path));
    let full_msrv = paths.is_empty()
        || classes.contains(&ChangeClass::Unknown)
        || paths.iter().any(|path| requires_full_msrv(path));
    let local_plan = if paths.is_empty() || paths.iter().any(|path| requires_full_local(path)) {
        LocalPlan::Full
    } else if docs_only {
        LocalPlan::Documentation
    } else {
        LocalPlan::Fast
    };
    Classification {
        docs_only,
        coverage,
        full_msrv,
        classes,
        local_plan,
    }
}

fn classify_path(path: &str, classes: &mut BTreeSet<ChangeClass>) {
    let predicates = [
        (ChangeClass::Documentation, has_extension(path, "md")),
        (ChangeClass::CiPolicy, is_ci_policy(path)),
        (ChangeClass::Dependencies, is_dependency(path)),
        (ChangeClass::Packaging, is_packaging(path)),
        (ChangeClass::Release, is_release(path)),
        (ChangeClass::Terminal, is_terminal(path)),
        (ChangeClass::PersistenceContract, is_persistence(path)),
        (
            ChangeClass::Product,
            path.starts_with("src/") || path.starts_with("tests/"),
        ),
        (
            ChangeClass::Tooling,
            path.starts_with("xtask/") || path.starts_with("tools/"),
        ),
    ];
    let recognized = predicates.iter().any(|(_, matches)| *matches);
    for (class, matches) in predicates {
        insert_if(classes, class, matches);
    }
    if !recognized || !is_known(path) {
        classes.insert(ChangeClass::Unknown);
    }
}

fn insert_if(classes: &mut BTreeSet<ChangeClass>, class: ChangeClass, condition: bool) {
    if condition {
        classes.insert(class);
    }
}

fn is_known(path: &str) -> bool {
    has_extension(path, "md")
        || path.starts_with("src/")
        || path.starts_with("tests/")
        || path.starts_with("xtask/")
        || path.starts_with("tools/")
        || path.starts_with("docs/")
        || path.starts_with("context/")
        || path.starts_with("assets/")
        || path.starts_with(".github/")
        || path.starts_with(".githooks/")
        || path.starts_with(".cargo/")
        || path.starts_with(".config/")
        || matches!(
            path,
            "AGENTS.md"
                | "CLAUDE.md"
                | "Cargo.toml"
                | "Cargo.lock"
                | "README.md"
                | "CONTRIBUTING.md"
                | "SECURITY.md"
                | "LICENSE"
                | "deny.toml"
                | "rust-toolchain.toml"
                | "rust-toolchain"
                | "rustfmt.toml"
                | "about.toml"
                | "about.hbs"
                | "dist-workspace.toml"
                | "release-highlights.json"
                | ".gitignore"
        )
}

fn is_ordinary_markdown(path: &str) -> bool {
    has_extension(path, "md") && !path.starts_with(".github/release-notes/")
}

fn is_ci_policy(path: &str) -> bool {
    path.starts_with(".github/workflows/")
        || path.starts_with(".github/actions/")
        || path == ".github/CODEOWNERS"
        || path.starts_with(".githooks/")
        || path.ends_with("AGENTS.md")
        || path.ends_with("CLAUDE.md")
        || matches!(path, "context/ARCHITECTURE.md" | "context/PRODUCT.md")
        || matches!(
            path,
            "xtask/src/ci_changes.rs"
                | "xtask/src/dev_gates.rs"
                | "xtask/src/gate_lock.rs"
                | "xtask/src/timing.rs"
                | "xtask/src/main.rs"
                | "xtask/src/release_policy.rs"
        )
}

fn is_dependency(path: &str) -> bool {
    matches!(
        path,
        "Cargo.toml"
            | "Cargo.lock"
            | "xtask/Cargo.toml"
            | "deny.toml"
            | "rust-toolchain.toml"
            | "rust-toolchain"
    ) || path.starts_with(".cargo/")
        || path.starts_with(".config/nextest")
}

fn is_packaging(path: &str) -> bool {
    path.starts_with("tools/ci-linux/")
        || path.starts_with("tests/package_contract")
        || matches!(path, "about.toml" | "about.hbs" | "dist-workspace.toml")
        || path.strip_prefix("xtask/src/").is_some_and(|name| {
            [
                "package",
                "crate_package",
                "debian",
                "linux_ci",
                "linux_compat",
            ]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        })
}

fn is_release(path: &str) -> bool {
    path.starts_with(".github/release-notes/")
        || path == "release-highlights.json"
        || path
            .strip_prefix("xtask/src/")
            .is_some_and(|name| name.starts_with("release") || name.starts_with("homebrew"))
}

fn is_terminal(path: &str) -> bool {
    path.starts_with("src/ui/")
        || path.starts_with("src/adapters/terminal/")
        || path.starts_with("src/adapters/process/")
        || path.starts_with("tests/pty")
        || matches!(
            path,
            "tests/ui_board.rs" | "tests/ui_mouse_actions.rs" | "tests/vt100_terminal.rs"
        )
}

fn is_persistence(path: &str) -> bool {
    path.starts_with("src/adapters/sqlite/")
        || path.starts_with("tests/sqlite_store")
        || matches!(
            path,
            "tests/recovery_export.rs" | "tests/state_path_safety.rs"
        )
}

fn requires_coverage(path: &str) -> bool {
    has_extension(path, "rs")
        || matches!(
            path,
            "Cargo.toml" | "Cargo.lock" | "rust-toolchain.toml" | "rust-toolchain"
        )
}

fn requires_full_msrv(path: &str) -> bool {
    is_dependency(path)
        || is_ci_policy(path)
        || is_packaging(path)
        || is_release(path)
        || path.ends_with("/Cargo.toml")
}

fn requires_full_local(path: &str) -> bool {
    !is_known(path)
        || is_ci_policy(path)
        || is_dependency(path)
        || is_packaging(path)
        || is_release(path)
}

fn has_extension(path: &str, extension: &str) -> bool {
    Path::new(path).extension() == Some(OsStr::new(extension))
}

#[cfg(test)]
#[path = "ci_changes_tests.rs"]
mod tests;
