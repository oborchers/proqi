//! Repository instruction-file ownership policy.

use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn check(root: &Path) -> Result<Vec<String>, String> {
    let mut agents = Vec::new();
    collect(root, &mut agents)?;
    let mut violations = Vec::new();
    for path in agents {
        let parent = path
            .parent()
            .ok_or_else(|| format!("instruction file has no parent: {}", path.display()))?;
        let claude = parent.join("CLAUDE.md");
        let metadata = fs::symlink_metadata(&claude);
        match metadata {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let target = fs::read_link(&claude)
                    .map_err(|error| format!("read {}: {error}", claude.display()))?;
                if target != Path::new("AGENTS.md") {
                    violations.push(format!(
                        "{}: CLAUDE.md must point to sibling AGENTS.md",
                        parent.strip_prefix(root).unwrap_or(parent).display()
                    ));
                }
            }
            Ok(_) => violations.push(format!(
                "{}: CLAUDE.md must be a relative symlink to AGENTS.md",
                parent.strip_prefix(root).unwrap_or(parent).display()
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => violations.push(format!(
                "{}: AGENTS.md is missing its CLAUDE.md symlink",
                parent.strip_prefix(root).unwrap_or(parent).display()
            )),
            Err(error) => return Err(format!("inspect {}: {error}", claude.display())),
        }
    }
    Ok(violations)
}

fn collect(directory: &Path, agents: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("read directory {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("read instruction entry: {error}"))?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|error| format!("read file type for {}: {error}", path.display()))?;
        if kind.is_dir() && !matches!(entry.file_name().to_str(), Some(".git" | "target")) {
            collect(&path, agents)?;
        } else if kind.is_file() && entry.file_name() == OsStr::new("AGENTS.md") {
            agents.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check;

    #[cfg(unix)]
    #[test]
    fn every_agents_file_requires_one_relative_claude_symlink() {
        use std::{fs, os::unix::fs::symlink};

        let fixture = tempfile::TempDir::new().expect("fixture");
        fs::write(fixture.path().join("AGENTS.md"), "instructions").expect("agents");
        assert_eq!(check(fixture.path()).expect("check").len(), 1);
        symlink("AGENTS.md", fixture.path().join("CLAUDE.md")).expect("symlink");
        assert!(check(fixture.path()).expect("check").is_empty());
    }
}
