//! Executable repository architecture policy.

use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

const INNER_FORBIDDEN: &[&str] = &[
    "crate::adapters",
    "crate::ui",
    "crossterm",
    "ratatui",
    "rusqlite",
    "std::env",
    "std::fs",
    "env::",
    "fs::",
    "process::Command",
];

pub(crate) fn check(root: &Path) -> Result<(), String> {
    let source_root = root.join("src");
    let mut files = Vec::new();
    collect_rust_files(&source_root, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err("architecture scan found no Rust source files".to_owned());
    }

    let mut layer_counts = [0_usize; 3];
    let mut violations = Vec::new();
    for path in &files {
        let relative = path.strip_prefix(root).unwrap_or(path);
        let source = fs::read_to_string(path)
            .map_err(|error| format!("read {}: {error}", relative.display()))?;
        classify(relative, &mut layer_counts);
        violations.extend(check_source(relative, &source));
    }
    if layer_counts.contains(&0) {
        return Err(format!(
            "architecture scan was incomplete: domain={}, application={}, ports={}",
            layer_counts[0], layer_counts[1], layer_counts[2]
        ));
    }
    if violations.is_empty() {
        println!(
            "architecture policy: checked {} Rust source files",
            files.len()
        );
        Ok(())
    } else {
        violations.sort();
        Err(format!(
            "architecture policy violations:\n{}",
            violations.join("\n")
        ))
    }
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("read directory {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("read directory entry: {error}"))?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|error| format!("read file type for {}: {error}", path.display()))?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension() == Some(OsStr::new("rs")) {
            files.push(path);
        }
    }
    Ok(())
}

fn classify(path: &Path, counts: &mut [usize; 3]) {
    let path = slash_path(path);
    if path.starts_with("src/domain/") {
        counts[0] += 1;
    } else if path.starts_with("src/application/") {
        counts[1] += 1;
    } else if path.starts_with("src/ports/") {
        counts[2] += 1;
    }
}

fn check_source(path: &Path, source: &str) -> Vec<String> {
    let path_text = slash_path(path);
    let mut violations = Vec::new();
    if path_text.starts_with("src/domain/") {
        find_markers(
            path,
            source,
            &["crate::application", "crate::ports"],
            &mut violations,
        );
        find_markers(path, source, INNER_FORBIDDEN, &mut violations);
    } else if path_text.starts_with("src/application/") || path_text.starts_with("src/ports/") {
        find_markers(path, source, INNER_FORBIDDEN, &mut violations);
    }
    enforce_adapter_ownership(path, source, &path_text, &mut violations);
    if path_text == "src/domain/mod.rs" && source.contains("pub mod ") {
        violations.push(format!(
            "{}: domain implementation modules must remain private",
            path.display()
        ));
    }
    violations
}

fn enforce_adapter_ownership(
    path: &Path,
    source: &str,
    path_text: &str,
    violations: &mut Vec<String>,
) {
    for (marker, allowed) in [
        ("rusqlite", &["src/adapters/sqlite/"][..]),
        ("crossterm", &["src/adapters/terminal/", "src/bin/"][..]),
        (
            "ratatui",
            &["src/ui/", "src/adapters/terminal/", "src/bin/"][..],
        ),
        (
            "process::Command",
            &["src/adapters/process/", "src/bin/"][..],
        ),
        ("std::env", &["src/adapters/", "src/bin/"][..]),
        ("std::fs", &["src/adapters/", "src/bin/"][..]),
        ("env::", &["src/adapters/", "src/bin/"][..]),
        ("fs::", &["src/adapters/", "src/bin/"][..]),
    ] {
        if source.contains(marker) && !allowed.iter().any(|prefix| path_text.starts_with(prefix)) {
            violations.push(format!(
                "{}: `{marker}` is outside its owning adapter",
                path.display()
            ));
        }
    }
}

fn find_markers(path: &Path, source: &str, markers: &[&str], violations: &mut Vec<String>) {
    for marker in markers {
        if source.contains(marker) {
            violations.push(format!(
                "{}: forbidden dependency `{marker}`",
                path.display()
            ));
        }
    }
}

fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_to_port_dependency_is_rejected() {
        let findings = check_source(
            Path::new("src/domain/model.rs"),
            "use crate::ports::editor::Editor;",
        );
        assert_eq!(findings.len(), 1);
        assert!(findings[0].contains("crate::ports"));
    }

    #[test]
    fn owned_adapter_dependency_is_accepted() {
        assert!(
            check_source(
                Path::new("src/adapters/sqlite/mod.rs"),
                "use rusqlite::Connection;"
            )
            .is_empty()
        );
    }

    #[test]
    fn misplaced_adapter_dependency_is_rejected() {
        let findings = check_source(Path::new("src/cli/mod.rs"), "use rusqlite::Connection;");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].contains("owning adapter"));
    }

    #[test]
    fn direct_filesystem_access_is_rejected_outside_adapters() {
        let findings = check_source(Path::new("src/cli/mod.rs"), "use std::fs;");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].contains("owning adapter"));
    }

    #[test]
    fn public_domain_implementation_module_is_rejected() {
        let findings = check_source(Path::new("src/domain/mod.rs"), "pub mod model;");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].contains("must remain private"));
    }
}
