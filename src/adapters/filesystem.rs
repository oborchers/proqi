//! Shared filesystem shape and private-directory preparation.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Resolve an existing directory to the physical path a launch would record.
pub(crate) fn canonical_existing_directory(path: &Path) -> io::Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    if fs::metadata(&canonical)?.is_dir() {
        Ok(canonical)
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "path is not a directory",
        ))
    }
}

/// Nearest ancestor of `path`, inclusive, that contains a `.git` entry.
pub(crate) fn repository_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .map(Path::to_path_buf)
}

pub(crate) fn validate_directory_path(path: &Path) -> io::Result<()> {
    if !path.is_absolute() {
        return Err(invalid_path(path, "must be absolute"));
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(invalid_path(path, "must not be a symbolic link"))
        }
        Ok(metadata) if !metadata.is_dir() => Err(invalid_path(path, "must be a directory")),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(crate) fn prepare_private_dir(path: &Path) -> io::Result<()> {
    validate_directory_path(path)?;
    fs::create_dir_all(path)?;
    validate_directory_path(path)?;
    make_private_dir(path)
}

pub(crate) fn prepare_private_dirs(paths: &[&Path]) -> io::Result<()> {
    for path in paths {
        validate_directory_path(path)?;
    }
    for path in paths {
        prepare_private_dir(path)?;
    }
    Ok(())
}

pub(crate) fn validate_file_path(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid_path(path, "has no parent directory"))?;
    validate_directory_path(parent)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(invalid_path(path, "must not be a symbolic link"))
        }
        Ok(metadata) if !metadata.is_file() => Err(invalid_path(path, "must be a regular file")),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn invalid_path(path: &Path, reason: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("unsafe Proqi state path {}: {reason}", path.display()),
    )
}

fn make_private_dir(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn directory_validation_rejects_a_symlink() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().expect("temporary directory");
        let target = temporary.path().join("target");
        fs::create_dir(&target).expect("target");
        let link = temporary.path().join("link");
        symlink(&target, &link).expect("symlink");
        let error = prepare_private_dir(&link).expect_err("unsafe directory");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(target.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn launch_directories_resolve_symlinks_and_reject_files_or_absence() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().expect("temporary directory");
        let target = temporary.path().join("target");
        fs::create_dir(&target).expect("target");
        let link = temporary.path().join("link");
        symlink(&target, &link).expect("symlink");
        let file = temporary.path().join("file");
        fs::write(&file, b"x").expect("file");

        let canonical = fs::canonicalize(&target).expect("canonical target");
        assert_eq!(
            canonical_existing_directory(&link).expect("link"),
            canonical
        );
        assert_eq!(
            canonical_existing_directory(&target).expect("target"),
            canonical
        );
        assert!(canonical_existing_directory(&file).is_err());
        assert!(canonical_existing_directory(&temporary.path().join("missing")).is_err());
    }
}
