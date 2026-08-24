//! Golden terminal snapshot policy.

use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn check(root: &Path) -> Result<(), String> {
    let snapshot_root = root.join("tests/ui_board/snapshots");
    let files = snapshot_files(&snapshot_root)?;
    let pending = files
        .iter()
        .filter(|path| path.extension().is_some_and(|extension| extension == "new"))
        .map(|path| relative(root, path))
        .collect::<Vec<_>>();
    if !pending.is_empty() {
        return Err(format!(
            "unreviewed terminal snapshots found:\n{}",
            pending.join("\n")
        ));
    }
    let accepted = files
        .iter()
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "snap")
        })
        .count();
    if accepted == 0 {
        return Err("no reviewed terminal UI snapshots were found".to_owned());
    }
    println!("snapshots: {accepted} reviewed terminal UI snapshots, no pending updates");
    Ok(())
}

fn snapshot_files(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("read snapshot directory {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("read snapshot entry: {error}"))?;
        if entry
            .file_type()
            .map_err(|error| format!("read snapshot file type: {error}"))?
            .is_file()
        {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_pending_and_accepted_snapshot_extensions() {
        let accepted = Path::new("board.snap");
        let pending = Path::new("board.snap.new");
        assert_eq!(accepted.extension(), Some("snap".as_ref()));
        assert_eq!(pending.extension(), Some("new".as_ref()));
    }

    #[test]
    fn renders_repository_relative_policy_paths() {
        let root = Path::new("/repo");
        assert_eq!(
            relative(root, Path::new("/repo/tests/board.snap.new")),
            "tests/board.snap.new"
        );
    }
}
