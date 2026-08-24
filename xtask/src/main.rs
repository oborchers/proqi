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

mod policy;
mod snapshots;

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_SOURCE_LINES: usize = 500;
const SOURCE_EXTENSIONS: &[&str] = &[
    "astro", "cjs", "css", "cts", "html", "js", "jsx", "less", "mjs", "mts", "rs", "scss",
    "svelte", "ts", "tsx", "vue",
];

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
        "source-limits" => check_source_limits(&root),
        "architecture" => policy::check(&root),
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
         \n  cargo xtask check\
         \n  cargo xtask test\
         \n  cargo xtask test-pty\
         \n  cargo xtask coverage\
         \n  cargo xtask audit\
         \n  cargo xtask package"
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
    ] {
        run(root, program, arguments.iter().copied())?;
    }
    Ok(())
}

fn check(root: &Path) -> Result<(), String> {
    run(root, "cargo", ["fmt", "--all", "--", "--check"])?;
    check_whitespace(root)?;
    check_source_limits(root)?;
    snapshots::check(root)?;
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

fn check_source_limits(root: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    collect_source_files(root, &mut files)?;
    files.sort();

    let mut violations = Vec::new();
    for path in files {
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let line_count = source.lines().count();
        if line_count > MAX_SOURCE_LINES {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            violations.push(format!("{}: {line_count} lines", relative.display()));
        }
    }

    if violations.is_empty() {
        println!(
            "source limits: every first-party source file is at most {MAX_SOURCE_LINES} lines"
        );
        Ok(())
    } else {
        Err(format!(
            "first-party source files exceed the {MAX_SOURCE_LINES}-line limit:\n{}",
            violations.join("\n")
        ))
    }
}

fn collect_source_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("read directory {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("read directory entry: {error}"))?;
        let path = entry.path();
        if is_impeccable_artifact(&path) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| format!("read file type for {}: {error}", path.display()))?;
        if file_type.is_dir() {
            let name = path.file_name().and_then(OsStr::to_str);
            if matches!(name, Some(".git" | "node_modules" | "target")) {
                continue;
            }
            collect_source_files(&path, files)?;
        } else if is_source_file(&path) && (file_type.is_file() || file_type.is_symlink()) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_impeccable_artifact(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| name.to_ascii_lowercase().contains("impeccable"))
    })
}

fn is_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| SOURCE_EXTENSIONS.contains(&extension))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_policy_covers_rust_and_common_frontend_languages() {
        for path in [
            "src/lib.rs",
            "ui/view.tsx",
            "ui/theme.css",
            "ui/page.svelte",
            "ui/component.vue",
            "ui/page.astro",
        ] {
            assert!(is_source_file(Path::new(path)), "uncovered source: {path}");
        }
        assert!(!is_source_file(Path::new("PRODUCT.md")));
    }

    #[test]
    fn configured_ceiling_is_inclusive() {
        assert_eq!("line\n".repeat(MAX_SOURCE_LINES).lines().count(), 500);
        assert_eq!("line\n".repeat(MAX_SOURCE_LINES + 1).lines().count(), 501);
    }

    #[test]
    fn local_impeccable_artifacts_are_not_first_party_source() {
        assert!(is_impeccable_artifact(Path::new(
            ".github/skills/impeccable/scripts/context.mjs"
        )));
        assert!(is_impeccable_artifact(Path::new(
            ".github/agents/impeccable-documenter.agent.md"
        )));
        assert!(!is_impeccable_artifact(Path::new("src/ui/render.rs")));
    }
}
