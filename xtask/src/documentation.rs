//! Reproducible user-documentation validation and static-site build.

use std::{path::Path, process::Command};

pub(super) fn build(root: &Path) -> Result<(), String> {
    super::run(
        root,
        "python3",
        [
            "-B",
            "-m",
            "unittest",
            "discover",
            "-s",
            "docs",
            "-p",
            "*_tests.py",
        ],
    )?;
    super::run(root, "python3", ["-B", "docs/check.py"])?;

    let environment = root.join("target/docs-venv");
    run_uv(
        root,
        &environment,
        ["sync", "--project", "docs", "--locked"],
    )?;
    run_uv(
        root,
        &environment,
        ["run", "--project", "docs", "mkdocs", "build", "--strict"],
    )?;

    super::run(root, "python3", ["-B", "docs/check_site.py"])
}

fn run_uv<const N: usize>(
    root: &Path,
    environment: &Path,
    arguments: [&str; N],
) -> Result<(), String> {
    println!("+ uv {}", arguments.join(" "));
    let status = Command::new("uv")
        .args(arguments)
        .env("UV_PROJECT_ENVIRONMENT", environment)
        .current_dir(root)
        .status()
        .map_err(|error| format!("start uv: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("uv exited with {status}"))
    }
}
