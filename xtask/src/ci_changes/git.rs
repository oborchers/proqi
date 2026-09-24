//! Git path collection for change classification.

use std::{
    ffi::OsStr,
    path::Path,
    process::{Command, Output},
};

pub(super) fn changed_paths(
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

pub(super) fn untracked_paths(root: &Path) -> Result<Vec<String>, String> {
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

pub(super) fn resolve_commit(root: &Path, revision: &str) -> Result<String, String> {
    command_text(
        root,
        ["rev-parse", "--verify", &format!("{revision}^{{commit}}")],
    )
    .map(|value| value.trim().to_owned())
}

pub(super) fn command_text<const N: usize>(
    root: &Path,
    arguments: [&str; N],
) -> Result<String, String> {
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
