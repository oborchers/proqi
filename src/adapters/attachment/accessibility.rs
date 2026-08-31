//! Read-only filesystem proof for annotated external attachment paths.

use std::{fs::File, io::Read as _, path::Path};

use rustix::fs::{Mode, OFlags};

use crate::ports::attachment_accessibility::{AttachmentAccessFailure, AttachmentAccessibility};

/// System filesystem implementation of binary attachment accessibility.
#[derive(Default)]
pub struct FileAttachmentAccessibility;

impl AttachmentAccessibility for FileAttachmentAccessibility {
    fn check(&mut self, path: &Path) -> Result<(), AttachmentAccessFailure> {
        if !path.is_absolute() {
            return Err(AttachmentAccessFailure::Unreadable);
        }
        let descriptor = rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(map_errno)?;
        let mut file = File::from(descriptor);
        let metadata = file.metadata().map_err(|error| map_io(&error))?;
        if !metadata.is_file() {
            return Err(AttachmentAccessFailure::Unreadable);
        }
        let mut probe = [0_u8; 1];
        let _bytes = file.read(&mut probe).map_err(|error| map_io(&error))?;
        Ok(())
    }
}

fn map_errno(error: rustix::io::Errno) -> AttachmentAccessFailure {
    if error == rustix::io::Errno::NOENT {
        AttachmentAccessFailure::Missing
    } else if error == rustix::io::Errno::ACCESS || error == rustix::io::Errno::PERM {
        AttachmentAccessFailure::PermissionDenied
    } else if error == rustix::io::Errno::NOTCONN {
        AttachmentAccessFailure::Unmounted
    } else {
        AttachmentAccessFailure::Io
    }
}

fn map_io(error: &std::io::Error) -> AttachmentAccessFailure {
    match error.kind() {
        std::io::ErrorKind::NotFound => AttachmentAccessFailure::Missing,
        std::io::ErrorKind::PermissionDenied => AttachmentAccessFailure::PermissionDenied,
        std::io::ErrorKind::NotConnected => AttachmentAccessFailure::Unmounted,
        std::io::ErrorKind::InvalidInput | std::io::ErrorKind::IsADirectory => {
            AttachmentAccessFailure::Unreadable
        }
        _ => AttachmentAccessFailure::Io,
    }
}

#[cfg(test)]
mod tests {
    use crate::ports::attachment_accessibility::{
        AttachmentAccessFailure, AttachmentAccessibility as _,
    };

    use super::FileAttachmentAccessibility;

    #[test]
    fn missing_directory_and_unicode_file_are_classified_without_rewriting_paths() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let unicode = temporary.path().join("Grüße 第一.txt");
        std::fs::write(&unicode, b"available").expect("unicode fixture");
        let mut accessibility = FileAttachmentAccessibility;
        assert_eq!(accessibility.check(&unicode), Ok(()));
        assert_eq!(
            accessibility.check(&temporary.path().join("missing.txt")),
            Err(AttachmentAccessFailure::Missing)
        );
        assert_eq!(
            accessibility.check(temporary.path()),
            Err(AttachmentAccessFailure::Unreadable)
        );
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_file_is_permission_denied_for_non_root_users() {
        use std::os::unix::fs::PermissionsExt as _;

        if rustix::process::geteuid().is_root() {
            return;
        }
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("private.txt");
        std::fs::write(&path, b"private").expect("fixture");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000))
            .expect("permissions");
        let result = FileAttachmentAccessibility.check(&path);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .expect("restore permissions");
        assert_eq!(result, Err(AttachmentAccessFailure::PermissionDenied));
    }

    #[test]
    fn unmounted_and_other_io_errors_remain_distinct_for_diagnostics() {
        assert_eq!(
            super::map_errno(rustix::io::Errno::NOTCONN),
            AttachmentAccessFailure::Unmounted
        );
        assert_eq!(
            super::map_errno(rustix::io::Errno::IO),
            AttachmentAccessFailure::Io
        );
    }
}
