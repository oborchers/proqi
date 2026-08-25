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

mod homebrew;
mod package;
mod policy;
mod public_assets;
mod release;
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

    match command.as_str() {
        "setup" => setup(&root),
        "install-hooks" => install_hooks(&root),
        "format" => run(&root, "cargo", ["fmt", "--all"]),
        "source-limits" => source_limits::check(&root),
        "architecture" => policy::check(&root),
        "assets" => public_assets::check(&root),
        "clean-worktree" => clean_worktree(&root),
        "check" => check(&root),
        "test" => test(&root),
        "test-pty" => run(
            &root,
            "cargo",
            ["test", "--workspace", "--all-features", "--test", "pty"],
        ),
        "coverage" => coverage(&root),
        "audit" => audit(&root),
        "package" => package::run(&root),
        "release-plan" => {
            let tag = env::args().nth(2);
            release::plan(&root, tag.as_deref())
        }
        "release-rehearsal" => release::rehearse(&root),
        "release-checksum" => {
            let path = env::args()
                .nth(2)
                .ok_or_else(|| "release-checksum requires one archive path".to_owned())?;
            release::print_checksum(&root, Path::new(&path))
        }
        "homebrew-formula" => {
            let artifacts = env::args()
                .nth(2)
                .ok_or_else(|| "homebrew-formula requires an artifacts directory".to_owned())?;
            let output = env::args()
                .nth(3)
                .ok_or_else(|| "homebrew-formula requires an output path".to_owned())?;
            homebrew::generate(&root, Path::new(&artifacts), Path::new(&output))
        }
        "msrv" => msrv(&root),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(format!("unknown command `{other}`; run `cargo xtask help`")),
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
         \n  cargo xtask check\
         \n  cargo xtask test\
         \n  cargo xtask test-pty\
         \n  cargo xtask coverage\
         \n  cargo xtask audit\
         \n  cargo xtask package\
         \n  cargo xtask release-plan [vX.Y.Z]\
         \n  cargo xtask release-rehearsal\
         \n  cargo xtask release-checksum <archive>\
         \n  cargo xtask homebrew-formula <artifacts-dir> <output>"
    );
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
    run(root, "cargo", ["fmt", "--all", "--", "--check"])?;
    check_whitespace(root)?;
    source_limits::check(root)?;
    snapshots::check(root)?;
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
    check_docs(root)?;
    test(root)
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
    )?;
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
            "--fail-under-lines",
            "70",
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
