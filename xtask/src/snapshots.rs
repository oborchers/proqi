//! Golden terminal snapshot policy.

use std::path::{Path, PathBuf};

#[cfg(test)]
use std::fs;

use ignore::WalkBuilder;

pub(crate) fn check(root: &Path) -> Result<(), String> {
    let files = snapshot_files(root)?;
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
    for entry in WalkBuilder::new(directory).standard_filters(true).build() {
        let entry = entry.map_err(|error| format!("walk snapshot files: {error}"))?;
        if entry.file_type().is_some_and(|kind| kind.is_file()) && is_snapshot_file(entry.path()) {
            files.push(entry.into_path());
        }
    }
    files.sort();
    Ok(files)
}

fn is_snapshot_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "snap")
        || (path.extension().is_some_and(|extension| extension == "new")
            && path
                .file_stem()
                .and_then(|stem| Path::new(stem).extension())
                .is_some_and(|extension| extension == "snap"))
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
        assert!(is_snapshot_file(accepted));
        assert!(is_snapshot_file(pending));
        assert!(!is_snapshot_file(Path::new("board.new")));
    }

    #[test]
    fn renders_repository_relative_policy_paths() {
        let root = Path::new("/repo");
        assert_eq!(
            relative(root, Path::new("/repo/tests/board.snap.new")),
            "tests/board.snap.new"
        );
    }

    #[test]
    fn discovers_snapshots_recursively_but_respects_ignored_directories() {
        let root = tempfile::tempdir().expect("temporary root");
        let nested = root.path().join("src/ui/snapshots");
        fs::create_dir_all(&nested).expect("nested snapshot directory");
        fs::write(nested.join("screen.snap"), "accepted").expect("accepted snapshot");
        fs::write(nested.join("screen.snap.new"), "pending").expect("pending snapshot");
        fs::write(nested.join("notes.txt"), "ordinary file").expect("ordinary file");

        let files = snapshot_files(root.path()).expect("snapshot scan");
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|path| path.starts_with(root.path())));
    }
}
