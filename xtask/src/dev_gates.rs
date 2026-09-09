//! Local iterative and final validation gates.

use std::{fs, path::Path, time::Instant};

use serde_json::json;

use super::ci_changes::{LocalChanges, LocalPlan};

const FULL_NEXTEST_ARGUMENTS: [&str; 5] = [
    "nextest",
    "run",
    "--locked",
    "--workspace",
    "--all-features",
];
const FAST_NEXTEST_ARGUMENTS: [&str; 7] = [
    "nextest",
    "run",
    "--locked",
    "--workspace",
    "--all-features",
    "-E",
    "not binary(=pty)",
];

pub(super) fn check(root: &Path, arguments: &[String]) -> Result<(), String> {
    let comparison = parse_check_arguments(arguments)?;
    match super::ci_changes::local(root, comparison.as_deref()) {
        Ok(changes) => run_classified(root, &changes),
        Err(error) => {
            println!("change classification failed closed: {error}");
            run_gate(
                root,
                GatePlan {
                    command: "check",
                    plan: "full",
                    selection_reason: "change classification failed, so the gate failed closed",
                    complete: true,
                    comparison: None,
                    classes: &[],
                    changed_paths: &[],
                    planned: &["cargo xtask quality", "cargo xtask test"],
                    omitted: &[],
                },
                || full_gate(root),
            )
        }
    }
}

pub(super) fn check_full(root: &Path, arguments: &[String]) -> Result<(), String> {
    if !arguments.is_empty() {
        return Err("check-full does not accept arguments".to_owned());
    }
    run_gate(
        root,
        GatePlan {
            command: "check-full",
            plan: "full",
            selection_reason: "explicit final qualification command",
            complete: true,
            comparison: None,
            classes: &[],
            changed_paths: &[],
            planned: &["cargo xtask quality", "cargo xtask test"],
            omitted: &[],
        },
        || full_gate(root),
    )
}

fn parse_check_arguments(arguments: &[String]) -> Result<Option<String>, String> {
    match arguments {
        [] => Ok(None),
        [flag, revision] if flag == "--base" && !revision.is_empty() => Ok(Some(revision.clone())),
        _ => Err("check accepts only `--base <revision>`".to_owned()),
    }
}

fn run_classified(root: &Path, changes: &LocalChanges) -> Result<(), String> {
    let classes = changes
        .classification
        .classes
        .iter()
        .map(|class| class.name())
        .collect::<Vec<_>>();
    println!("iterative check base: {}", changes.base);
    println!("iterative check head: {}", changes.head);
    println!("changed paths: {}", changes.paths.join(", "));
    println!("change classes: {}", classes.join(", "));
    println!(
        "selected local plan: {}",
        changes.classification.local_plan.name()
    );
    let comparison = Some((changes.base.as_str(), changes.head.as_str()));
    match changes.classification.local_plan {
        LocalPlan::Documentation => run_gate(
            root,
            GatePlan {
                command: "check",
                plan: "documentation",
                selection_reason: "every changed path is ordinary Markdown and no path owns policy",
                complete: false,
                comparison,
                classes: &classes,
                changed_paths: &changes.paths,
                planned: &["changed Markdown whitespace", "cargo xtask assets"],
                omitted: &["quality", "nextest", "PTY", "doctests"],
            },
            || documentation_gate(root, changes),
        ),
        LocalPlan::Fast => run_gate(
            root,
            GatePlan {
                command: "check",
                plan: "fast",
                selection_reason: "changed paths are known product or tooling paths without a high-risk class",
                complete: false,
                comparison,
                classes: &classes,
                changed_paths: &changes.paths,
                planned: &[
                    "cargo xtask quality",
                    "cargo nextest run excluding binary(=pty)",
                    "cargo test --doc",
                ],
                omitted: &["PTY integration binary"],
            },
            || {
                quality(root)?;
                test_fast(root)
            },
        ),
        LocalPlan::Full => run_gate(
            root,
            GatePlan {
                command: "check",
                plan: "full",
                selection_reason: "a high-risk or ambiguous change class requires the complete gate",
                complete: true,
                comparison,
                classes: &classes,
                changed_paths: &changes.paths,
                planned: &["cargo xtask quality", "cargo xtask test"],
                omitted: &[],
            },
            || full_gate(root),
        ),
    }
}

#[derive(Clone, Copy)]
struct GatePlan<'a> {
    command: &'a str,
    plan: &'a str,
    selection_reason: &'a str,
    complete: bool,
    comparison: Option<(&'a str, &'a str)>,
    classes: &'a [&'a str],
    changed_paths: &'a [String],
    planned: &'a [&'a str],
    omitted: &'a [&'a str],
}

fn run_gate(
    root: &Path,
    plan: GatePlan<'_>,
    operation: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let started = Instant::now();
    println!("planned commands: {}", plan.planned.join("; "));
    println!("selection reason: {}", plan.selection_reason);
    if !plan.complete {
        println!("ITERATIVE GATE ONLY: final qualification requires `cargo xtask check-full`");
    }
    let result = operation();
    let elapsed_ms = started.elapsed().as_millis();
    let outcome = if result.is_ok() { "success" } else { "failure" };
    let receipt = json!({
        "schema_version": 1,
        "command": plan.command,
        "plan": plan.plan,
        "selection_reason": plan.selection_reason,
        "base_sha": plan.comparison.map(|(base, _)| base),
        "head_sha": plan.comparison.map(|(_, head)| head),
        "classes": plan.classes,
        "changed_paths": plan.changed_paths,
        "planned_commands": plan.planned,
        "intentionally_omitted": plan.omitted,
        "outcome": outcome,
        "error": result.as_ref().err(),
        "elapsed_ms": elapsed_ms,
        "complete_gate": plan.complete,
        "final_qualification": plan.complete && result.is_ok(),
    });
    let receipt_result = write_receipt(root, &receipt);
    println!("check receipt: target/xtask/check-receipt.json");
    println!(
        "check outcome: {outcome}; elapsed_ms={elapsed_ms}; complete_gate={}",
        plan.complete
    );
    combine_results(result, receipt_result)
}

fn combine_results(
    operation: Result<(), String>,
    receipt: Result<(), String>,
) -> Result<(), String> {
    match (operation, receipt) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(operation), Err(receipt)) => Err(format!("{operation}; additionally {receipt}")),
    }
}

fn write_receipt(root: &Path, receipt: &serde_json::Value) -> Result<(), String> {
    let directory = root.join("target/xtask");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("create check receipt directory: {error}"))?;
    let contents = serde_json::to_string_pretty(receipt)
        .map_err(|error| format!("render check receipt: {error}"))?;
    fs::write(
        directory.join("check-receipt.json"),
        format!("{contents}\n"),
    )
    .map_err(|error| format!("write check receipt: {error}"))
}

fn documentation_gate(root: &Path, changes: &LocalChanges) -> Result<(), String> {
    super::timing::phase("check.documentation.whitespace", || {
        super::run(
            root,
            "git",
            ["diff", "--check", changes.base.as_str(), "--"],
        )?;
        check_untracked_whitespace(root, changes)
    })?;
    super::timing::phase("check.documentation.assets", || {
        super::public_assets::check(root)
    })
}

fn check_untracked_whitespace(root: &Path, changes: &LocalChanges) -> Result<(), String> {
    for relative in &changes.untracked {
        let path = root.join(relative);
        if !path.is_file() {
            continue;
        }
        let contents = fs::read_to_string(&path)
            .map_err(|error| format!("read untracked text {relative}: {error}"))?;
        for (index, line) in contents.lines().enumerate() {
            if line.ends_with(' ') || line.ends_with('\t') {
                return Err(format!("{relative}:{}: trailing whitespace", index + 1));
            }
        }
    }
    Ok(())
}

fn full_gate(root: &Path) -> Result<(), String> {
    let _lease = super::timing::phase("check-full.lock", || super::gate_lock::acquire(root))?;
    quality(root)?;
    test(root)
}

pub(super) fn quality(root: &Path) -> Result<(), String> {
    super::timing::phase("quality.format", || {
        super::run(root, "cargo", ["fmt", "--all", "--", "--check"])
    })?;
    super::timing::phase("quality.whitespace", || check_whitespace(root))?;
    super::timing::phase("quality.source_limits", || {
        super::source_limits::check(root)
    })?;
    super::timing::phase("quality.snapshots", || super::snapshots::check(root))?;
    super::timing::phase("quality.release_highlights", || {
        super::release_highlights::validate(root, None)
    })?;
    super::timing::phase("quality.assets", || super::public_assets::check(root))?;
    super::timing::phase("quality.architecture", || super::policy::check(root))?;
    super::timing::phase("quality.clippy", || {
        super::run(
            root,
            "cargo",
            [
                "clippy",
                "--locked",
                "--workspace",
                "--all-targets",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ],
        )
    })?;
    super::timing::phase("quality.rustdoc", || check_docs(root))
}

fn check_whitespace(root: &Path) -> Result<(), String> {
    super::run(root, "git", ["diff", "--check"])?;
    super::run(root, "git", ["diff", "--cached", "--check"])?;
    super::run(root, "git", ["show", "--check", "--format=", "HEAD"])
}

fn check_docs(root: &Path) -> Result<(), String> {
    let arguments = [
        "doc",
        "--locked",
        "--workspace",
        "--all-features",
        "--no-deps",
    ];
    println!("+ RUSTDOCFLAGS=-D warnings cargo {}", arguments.join(" "));
    let status = std::process::Command::new("cargo")
        .args(arguments)
        .env("RUSTDOCFLAGS", "-D warnings")
        .current_dir(root)
        .status()
        .map_err(|error| format!("start cargo doc: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("cargo doc exited with {status}"))
}

pub(super) fn test(root: &Path) -> Result<(), String> {
    super::timing::phase("test.nextest", || {
        super::run(root, "cargo", FULL_NEXTEST_ARGUMENTS)
    })?;
    super::timing::phase("test.doctests", || run_doctests(root))
}

fn test_fast(root: &Path) -> Result<(), String> {
    super::timing::phase("test-fast.nextest", || {
        super::run(root, "cargo", FAST_NEXTEST_ARGUMENTS)
    })?;
    super::timing::phase("test-fast.doctests", || run_doctests(root))
}

fn run_doctests(root: &Path) -> Result<(), String> {
    super::run(
        root,
        "cargo",
        ["test", "--locked", "--workspace", "--all-features", "--doc"],
    )
}

#[cfg(test)]
mod tests {
    use super::{
        FAST_NEXTEST_ARGUMENTS, FULL_NEXTEST_ARGUMENTS, check_untracked_whitespace,
        parse_check_arguments,
    };
    use crate::ci_changes::{Classification, LocalChanges, LocalPlan};
    use std::collections::BTreeSet;

    #[test]
    fn check_arguments_are_bounded() {
        assert_eq!(parse_check_arguments(&[]).expect("default"), None);
        assert_eq!(
            parse_check_arguments(&["--base".to_owned(), "main".to_owned()]).expect("base"),
            Some("main".to_owned())
        );
        assert!(parse_check_arguments(&["--other".to_owned()]).is_err());
    }

    #[test]
    fn iterative_tests_omit_only_the_pty_binary() {
        assert_eq!(
            &FAST_NEXTEST_ARGUMENTS[..FULL_NEXTEST_ARGUMENTS.len()],
            FULL_NEXTEST_ARGUMENTS
        );
        assert_eq!(
            &FAST_NEXTEST_ARGUMENTS[FULL_NEXTEST_ARGUMENTS.len()..],
            ["-E", "not binary(=pty)"]
        );
    }

    #[test]
    fn untracked_document_whitespace_is_checked() {
        let root = tempfile::tempdir().expect("root");
        std::fs::write(root.path().join("report.md"), "good\nbad \n").expect("report");
        let changes = LocalChanges {
            base: "base".to_owned(),
            head: "head".to_owned(),
            paths: vec!["report.md".to_owned()],
            untracked: BTreeSet::from(["report.md".to_owned()]),
            classification: Classification {
                docs_only: true,
                coverage: false,
                full_msrv: false,
                classes: BTreeSet::new(),
                local_plan: LocalPlan::Documentation,
            },
        };
        assert!(check_untracked_whitespace(root.path(), &changes).is_err());
    }
}
