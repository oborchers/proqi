//! Repository-owned development, quality, and packaging commands.

#![cfg_attr(
    not(test),
    deny(
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::unreachable,
        clippy::unwrap_used
    )
)]

mod ci_changes;
mod crate_package;
mod debian;
mod debian_container;
mod debian_verify;
mod homebrew;
mod instructions;
mod linux_ci;
mod linux_compat;
mod package;
mod policy;
mod public_assets;
mod release;
mod release_candidate;
mod release_highlights;
mod release_policy;
mod release_publication;
mod release_readiness;
mod release_targets;
mod snapshots;
mod source_limits;

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn execute() -> Result<(), String> {
    let command = env::args().nth(1).unwrap_or_else(|| "help".to_owned());
    let root = workspace_root()?;
    if let Some(result) = release_command(&root, &command) {
        return result;
    }

    match command.as_str() {
        "setup" => setup(&root),
        "install-hooks" => install_hooks(&root),
        "format" => run(&root, "cargo", ["fmt", "--all"]),
        "source-limits" => source_limits::check(&root),
        "architecture" => policy::check(&root),
        "assets" => public_assets::check(&root),
        "clean-worktree" => clean_worktree(&root),
        "quality" => quality(&root),
        "check" => check(&root),
        "test" => test(&root),
        "ci-linux" => linux_ci::run_local(&root),
        "ci-linux-smoke" => {
            let image = required_argument("ci-linux-smoke", 2, "digest-pinned image")?;
            linux_ci::run_prebuilt(&root, &image, None, "smoke")
        }
        "ci-linux-amd64" => {
            let image = required_argument("ci-linux-amd64", 2, "digest-pinned image")?;
            linux_ci::run_prebuilt(&root, &image, Some("linux/amd64"), "parity")
        }
        // Real PTY fixtures own process-wide terminal resources. Keep this
        // dedicated runner serial while each test still exercises concurrency.
        "test-pty" => run(
            &root,
            "cargo",
            [
                "test",
                "--workspace",
                "--all-features",
                "--test",
                "pty",
                "--",
                "--test-threads=1",
            ],
        ),
        "coverage" => coverage(&root),
        "audit" => audit(&root),
        "package" => {
            let notices = package_notices_argument()?;
            package::run(&root, notices.as_deref())
        }
        "crate-package" => crate_package::run(&root),
        "crate-evidence" => crate_package::evidence(&root),
        "ci-change-class" => required_argument("ci-change-class", 2, "base SHA").and_then(|base| {
            let head = required_argument("ci-change-class", 3, "head SHA")?;
            ci_changes::print(&root, &base, &head)
        }),
        "debian-package" => {
            let archive = required_path_argument("debian-package", 2, "Linux archive")?;
            let output = required_path_argument("debian-package", 3, "output directory")?;
            debian::package(&root, &archive, &output)
        }
        "verify-debian" => {
            let archive = required_path_argument("verify-debian", 2, "Linux archive")?;
            let package = required_path_argument("verify-debian", 3, "Debian package")?;
            debian_container::verify(&root, &archive, &package)
        }
        "msrv" => msrv(&root),
        "msrv-full" => msrv_full(&root),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(format!("unknown command `{other}`; run `cargo xtask help`")),
    }
}

fn release_command(root: &Path, command: &str) -> Option<Result<(), String>> {
    let result = match command {
        "release-plan" => {
            let tag = env::args().nth(2);
            release::plan(root, tag.as_deref())
        }
        "release-ready" => {
            let source_sha = env::args().nth(2);
            release_readiness::print_classification(root, source_sha.as_deref())
        }
        "release-promotion-plan" => required_argument("release-promotion-plan", 2, "vX.Y.Z")
            .and_then(|tag| release_readiness::validate_promotion(root, &tag)),
        "candidate-select" => required_argument("candidate-select", 2, "tag").and_then(|tag| {
            let sha = required_argument("candidate-select", 3, "source SHA")?;
            let index = required_path_argument("candidate-select", 4, "candidate index")?;
            release_candidate::select(root, &tag, &sha, &index)
        }),
        "candidate-manifest" => required_argument("candidate-manifest", 2, "create or verify")
            .and_then(|operation| release_candidate::manifest_command(root, &operation)),
        "release-assets" => required_argument("release-assets", 2, "plan operation").and_then(
            |operation| {
                if operation != "plan" {
                    return Err("release-assets expects `plan <candidate-dir> <existing-dir> <release-state.json>`".to_owned());
                }
                let candidate = required_path_argument("release-assets plan", 3, "candidate directory")?;
                let existing = required_path_argument("release-assets plan", 4, "existing assets directory")?;
                let state = required_path_argument("release-assets plan", 5, "release state JSON")?;
                release_publication::plan(root, &candidate, &existing, &state)
            },
        ),
        "release-rehearsal" => release::rehearse(root),
        "release-checksum" => required_path_argument("release-checksum", 2, "archive path")
            .and_then(|path| release::print_checksum(root, &path)),
        "verify-linux-archive" => required_path_argument("verify-linux-archive", 2, "archive path")
            .and_then(|path| linux_compat::verify_archive(root, &path)),
        "homebrew-formula" => required_path_argument("homebrew-formula", 2, "artifacts directory")
            .and_then(|artifacts| {
                let output = required_path_argument("homebrew-formula", 3, "output path")?;
                homebrew::generate(root, &artifacts, &output)
            }),
        _ => return None,
    };
    Some(result)
}

fn package_notices_argument() -> Result<Option<PathBuf>, String> {
    let arguments = env::args().skip(2).collect::<Vec<_>>();
    match arguments.as_slice() {
        [] => Ok(None),
        [flag, path] if flag == "--notices" => Ok(Some(PathBuf::from(path))),
        _ => Err("package accepts only `--notices <path>`".to_owned()),
    }
}

fn workspace_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask has no workspace parent".to_owned())
}

fn print_help() {
    println!(
        "Proqi development tasks:\n\
         \n  cargo xtask setup\
         \n  cargo xtask install-hooks\
         \n  cargo xtask format\
         \n  cargo xtask source-limits\
         \n  cargo xtask architecture\
         \n  cargo xtask assets\
         \n  cargo xtask clean-worktree\
         \n  cargo xtask quality\
         \n  cargo xtask check\
         \n  cargo xtask test\
         \n  cargo xtask ci-linux\
         \n  cargo xtask ci-linux-smoke <image@sha256:digest>\
         \n  cargo xtask ci-linux-amd64 <image@sha256:digest>\
         \n  cargo xtask test-pty\
         \n  cargo xtask coverage\
         \n  cargo xtask audit\
         \n  cargo xtask package\
         \n  cargo xtask crate-package\
         \n  cargo xtask crate-evidence\
         \n  cargo xtask ci-change-class <base-sha> <head-sha>\
         \n  cargo xtask debian-package <linux-archive> <output-dir>\
         \n  cargo xtask verify-debian <linux-archive> <deb>\
         \n  cargo xtask release-plan [vX.Y.Z]\
         \n  cargo xtask release-ready [source-sha]\
         \n  cargo xtask release-promotion-plan <vX.Y.Z>\
         \n  cargo xtask candidate-select <vX.Y.Z> <source-sha> <index.json>\
         \n  cargo xtask candidate-manifest <create|verify> ...\
         \n  cargo xtask release-assets plan <candidate-dir> <existing-dir> <release-state.json>\
         \n  cargo xtask release-rehearsal\
         \n  cargo xtask release-checksum <archive>\
         \n  cargo xtask verify-linux-archive <archive>\
         \n  cargo xtask homebrew-formula <artifacts-dir> <output>\
         \n  cargo xtask msrv\
         \n  cargo xtask msrv-full"
    );
}

fn required_argument(command: &str, index: usize, description: &str) -> Result<String, String> {
    env::args()
        .nth(index)
        .ok_or_else(|| format!("{command} requires a {description}"))
}

fn required_path_argument(
    command: &str,
    index: usize,
    description: &str,
) -> Result<PathBuf, String> {
    env::args()
        .nth(index)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{command} requires a {description}"))
}

fn install_hooks(root: &Path) -> Result<(), String> {
    run(
        root,
        "git",
        ["config", "--local", "core.hooksPath", ".githooks"],
    )
}

fn setup(root: &Path) -> Result<(), String> {
    for (program, arguments) in [
        ("rustc", &["--version"][..]),
        ("cargo", &["--version"][..]),
        ("cargo", &["nextest", "--version"][..]),
        ("cargo", &["llvm-cov", "--version"][..]),
        ("cargo", &["deny", "--version"][..]),
        ("cargo", &["audit", "--version"][..]),
        ("cargo", &["shear", "--version"][..]),
    ] {
        run(root, program, arguments.iter().copied())?;
    }
    Ok(())
}

fn check(root: &Path) -> Result<(), String> {
    quality(root)?;
    test(root)
}

fn quality(root: &Path) -> Result<(), String> {
    run(root, "cargo", ["fmt", "--all", "--", "--check"])?;
    check_whitespace(root)?;
    source_limits::check(root)?;
    snapshots::check(root)?;
    release_highlights::validate(root, None)?;
    public_assets::check(root)?;
    policy::check(root)?;
    run(
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
    )?;
    check_docs(root)
}

fn check_whitespace(root: &Path) -> Result<(), String> {
    run(root, "git", ["diff", "--check"])?;
    run(root, "git", ["diff", "--cached", "--check"])?;
    run(root, "git", ["show", "--check", "--format=", "HEAD"])
}

fn clean_worktree(root: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("start git status: {error}"))?;
    if !output.status.success() {
        return Err(format!("git status exited with {}", output.status));
    }
    let changes = String::from_utf8(output.stdout)
        .map_err(|error| format!("git status returned non-UTF-8 output: {error}"))?;
    changes.is_empty().then_some(()).ok_or_else(|| {
        format!(
            "quality commands changed the checkout:\n{}",
            changes.trim_end()
        )
    })
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
    let status = Command::new("cargo")
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

fn msrv(root: &Path) -> Result<(), String> {
    run(
        root,
        "cargo",
        [
            "check",
            "--locked",
            "--workspace",
            "--all-targets",
            "--all-features",
        ],
    )
}

fn msrv_full(root: &Path) -> Result<(), String> {
    msrv(root)?;
    run(
        root,
        "cargo",
        ["test", "--locked", "--workspace", "--all-features"],
    )
}

fn test(root: &Path) -> Result<(), String> {
    run(
        root,
        "cargo",
        [
            "nextest",
            "run",
            "--locked",
            "--workspace",
            "--all-features",
        ],
    )?;
    run(
        root,
        "cargo",
        ["test", "--locked", "--workspace", "--all-features", "--doc"],
    )
}

fn coverage(root: &Path) -> Result<(), String> {
    const PRODUCT_COVERAGE_FLOOR: &str = "70";
    // Workspace tests still run; the report measures the shipped product, not this build driver.
    const NON_SHIPPED_TOOLING: &str = "(^|/)xtask/";

    fs::create_dir_all(root.join("target/coverage"))
        .map_err(|error| format!("create coverage directory: {error}"))?;
    run(
        root,
        "cargo",
        [
            "llvm-cov",
            "nextest",
            "--locked",
            "--workspace",
            "--all-features",
            "--lcov",
            "--output-path",
            "target/coverage/lcov.info",
            "--ignore-filename-regex",
            NON_SHIPPED_TOOLING,
            "--fail-under-lines",
            PRODUCT_COVERAGE_FLOOR,
        ],
    )
}

fn audit(root: &Path) -> Result<(), String> {
    run(root, "cargo", ["deny", "check"])?;
    run(root, "cargo", ["audit", "--deny", "warnings"])?;
    run(root, "cargo", ["shear", "--deny-warnings"])
}

fn run<I, S>(cwd: &Path, program: impl AsRef<OsStr>, arguments: I) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let arguments: Vec<_> = arguments
        .into_iter()
        .map(|argument| argument.as_ref().to_owned())
        .collect();
    let rendered = arguments
        .iter()
        .map(|argument| argument.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    println!("+ {} {rendered}", program.as_ref().to_string_lossy());

    let status = Command::new(&program)
        .args(&arguments)
        .current_dir(cwd)
        .status()
        .map_err(|error| format!("start {}: {error}", program.as_ref().to_string_lossy()))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} exited with {status}",
            program.as_ref().to_string_lossy()
        ))
    }
}
