//! Secure, bounded local-control endpoint paths.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::domain::InstanceId;

use super::{RuntimeError, io_error};

#[cfg(unix)]
const MAX_UNIX_ENDPOINT_BYTES: usize = 100;

pub(super) fn prepare(
    runtime_dir: &Path,
    instance_id: InstanceId,
) -> Result<Option<String>, RuntimeError> {
    let endpoint = endpoint_path(runtime_dir, instance_id)?;
    let parent = endpoint
        .parent()
        .ok_or_else(|| RuntimeError::Invalid("control endpoint has no parent".to_owned()))?;
    prepare_owned_directory(parent, owner_uid(runtime_dir)?)?;
    Ok(Some(endpoint.to_string_lossy().into_owned()))
}

pub(super) fn existing(
    runtime_dir: &Path,
    instance_id: InstanceId,
) -> Result<Option<PathBuf>, RuntimeError> {
    let endpoint = endpoint_path(runtime_dir, instance_id)?;
    let Some(parent) = endpoint.parent() else {
        return Ok(None);
    };
    if validate_owned_directory(parent, owner_uid(runtime_dir)?).is_err() {
        return Ok(None);
    }
    Ok(Some(endpoint))
}

fn endpoint_path(runtime_dir: &Path, instance_id: InstanceId) -> Result<PathBuf, RuntimeError> {
    use std::os::unix::fs::MetadataExt as _;

    let local = runtime_dir
        .join("control")
        .join(format!("{instance_id}.sock"));
    if local.as_os_str().as_encoded_bytes().len() <= MAX_UNIX_ENDPOINT_BYTES {
        return Ok(local);
    }
    let uid = fs::metadata(runtime_dir).map_err(io_error)?.uid();
    Ok(std::env::temp_dir()
        .join(format!("proqi-{uid}"))
        .join(format!("{instance_id}.sock")))
}

fn owner_uid(path: &Path) -> Result<u32, RuntimeError> {
    use std::os::unix::fs::MetadataExt as _;
    Ok(fs::metadata(path).map_err(io_error)?.uid())
}

fn prepare_owned_directory(path: &Path, owner_uid: u32) -> Result<(), RuntimeError> {
    match fs::symlink_metadata(path) {
        Ok(_) => validate_owned_directory(path, owner_uid),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            create_private_directory(path)?;
            validate_owned_directory(path, owner_uid)
        }
        Err(error) => Err(io_error(error)),
    }
}

fn create_private_directory(path: &Path) -> Result<(), RuntimeError> {
    use std::os::unix::fs::DirBuilderExt as _;
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700);
    builder.create(path).map_err(io_error)
}

fn validate_owned_directory(path: &Path, owner_uid: u32) -> Result<(), RuntimeError> {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    let valid = metadata.file_type().is_dir()
        && !metadata.file_type().is_symlink()
        && metadata.uid() == owner_uid
        && metadata.permissions().mode() & 0o777 == 0o700;
    valid.then_some(()).ok_or_else(|| {
        RuntimeError::Invalid(format!(
            "control endpoint directory is not a private owned directory: {}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::{owner_uid, prepare_owned_directory};
    use tempfile::tempdir;

    #[test]
    fn refuses_a_symlinked_endpoint_parent() {
        let temporary = tempdir().expect("temporary directory");
        let runtime = temporary.path().join("runtime");
        let victim = temporary.path().join("victim");
        std::fs::create_dir(&runtime).expect("runtime directory");
        std::fs::create_dir(&victim).expect("victim directory");
        let victim_permissions = std::fs::metadata(&victim)
            .expect("victim metadata")
            .permissions();
        symlink(&victim, runtime.join("control")).expect("control symlink");
        assert!(
            prepare_owned_directory(
                &runtime.join("control"),
                owner_uid(&runtime).expect("owner")
            )
            .is_err()
        );
        assert_eq!(
            std::fs::metadata(&victim)
                .expect("victim metadata")
                .permissions(),
            victim_permissions
        );
    }
}
