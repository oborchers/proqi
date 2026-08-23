//! Repository-owned development, quality, and packaging commands.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

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
        "format" => run(&root, "cargo", ["fmt", "--all"]),
        "check" => check(&root),
        "test" => test(&root),
        "test-pty" => run(
            &root,
            "cargo",
            ["test", "--workspace", "--all-features", "--test", "pty"],
        ),
        "coverage" => coverage(&root),
        "audit" => audit(&root),
        "package" => package(&root),
        "msrv" => run(
            &root,
            "cargo",
            [
                "check",
                "--locked",
                "--workspace",
                "--all-targets",
                "--all-features",
            ],
        ),
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
         \n  cargo xtask format\
         \n  cargo xtask check\
         \n  cargo xtask test\
         \n  cargo xtask test-pty\
         \n  cargo xtask coverage\
         \n  cargo xtask audit\
         \n  cargo xtask package"
    );
}

fn setup(root: &Path) -> Result<(), String> {
    for (program, arguments) in [
        ("rustc", &["--version"][..]),
        ("cargo", &["--version"][..]),
        ("cargo", &["nextest", "--version"][..]),
        ("cargo", &["llvm-cov", "--version"][..]),
        ("cargo", &["deny", "--version"][..]),
        ("cargo", &["audit", "--version"][..]),
    ] {
        run(root, program, arguments.iter().copied())?;
    }
    Ok(())
}

fn check(root: &Path) -> Result<(), String> {
    run(root, "cargo", ["fmt", "--all", "--", "--check"])?;
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
    test(root)
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
        ],
    )
}

fn audit(root: &Path) -> Result<(), String> {
    run(root, "cargo", ["deny", "check"])?;
    run(root, "cargo", ["audit", "--deny", "warnings"])
}

fn package(root: &Path) -> Result<(), String> {
    run(
        root,
        "cargo",
        [
            "build",
            "--locked",
            "--workspace",
            "--all-features",
            "--release",
        ],
    )?;

    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("read system clock: {error}"))?
        .as_nanos();
    let prefix = env::temp_dir().join(format!("proqi-install-{suffix}"));
    let bin_dir = prefix.join("bin");
    fs::create_dir_all(&bin_dir).map_err(|error| format!("create install prefix: {error}"))?;

    let executable_name = if cfg!(windows) { "proqi.exe" } else { "proqi" };
    let source = root.join("target/release").join(executable_name);
    let installed = bin_dir.join(executable_name);
    fs::copy(&source, &installed)
        .map_err(|error| format!("install {}: {error}", source.display()))?;

    let result = run(&prefix, installed.as_os_str(), ["--version"]);
    let cleanup = fs::remove_dir_all(&prefix)
        .map_err(|error| format!("remove temporary install prefix: {error}"));
    result.and(cleanup)
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
