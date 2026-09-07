//! Read-only filesystem proof for annotated external attachment paths.

use std::{fs::File, io::Read as _, path::Path};

use rustix::fs::{Mode, OFlags};

use crate::ports::attachment_accessibility::{
    AttachmentAccessFailure, AttachmentAccessibility, AttachmentAvailability,
};

#[cfg(target_os = "macos")]
mod macos;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CloudDownloadStatus {
    NotDownloaded,
    Downloaded,
    Current,
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CloudMetadata {
    ubiquitous: Option<bool>,
    downloading: Option<bool>,
    status: Option<CloudDownloadStatus>,
    has_download_error: bool,
}

trait CloudMetadataProbe {
    fn metadata(&self, path: &Path) -> Option<CloudMetadata>;
}

#[cfg(not(target_os = "macos"))]
struct PlatformCloudMetadata;

#[cfg(not(target_os = "macos"))]
impl CloudMetadataProbe for PlatformCloudMetadata {
    fn metadata(&self, _path: &Path) -> Option<CloudMetadata> {
        None
    }
}

#[cfg(target_os = "macos")]
use macos::PlatformCloudMetadata;

/// System filesystem implementation of typed attachment availability.
#[derive(Default)]
pub struct FileAttachmentAccessibility;

impl AttachmentAccessibility for FileAttachmentAccessibility {
    fn check(&mut self, path: &Path) -> Result<AttachmentAvailability, AttachmentAccessFailure> {
        check_with_metadata(path, &PlatformCloudMetadata)
    }
}

fn check_with_metadata(
    path: &Path,
    cloud: &impl CloudMetadataProbe,
) -> Result<AttachmentAvailability, AttachmentAccessFailure> {
    if !path.is_absolute() {
        return Err(AttachmentAccessFailure::Unreadable);
    }
    if let Some(metadata) = cloud.metadata(path)
        && metadata.ubiquitous == Some(true)
        && !metadata.has_download_error
    {
        match (metadata.downloading, metadata.status) {
            (
                Some(true),
                Some(CloudDownloadStatus::NotDownloaded | CloudDownloadStatus::Downloaded),
            ) => return Ok(AttachmentAvailability::Downloading),
            (Some(false), Some(CloudDownloadStatus::NotDownloaded)) => {
                return Ok(AttachmentAvailability::InCloud);
            }
            _ => {}
        }
    }
    prove_exact_readability(path)
}

fn prove_exact_readability(path: &Path) -> Result<AttachmentAvailability, AttachmentAccessFailure> {
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
    Ok(AttachmentAvailability::Available)
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
        AttachmentAccessFailure, AttachmentAccessibility as _, AttachmentAvailability,
    };

    use super::{
        CloudDownloadStatus, CloudMetadata, CloudMetadataProbe, FileAttachmentAccessibility,
        check_with_metadata,
    };

    struct FixedMetadata(Option<CloudMetadata>);

    impl CloudMetadataProbe for FixedMetadata {
        fn metadata(&self, _path: &std::path::Path) -> Option<CloudMetadata> {
            self.0
        }
    }

    #[test]
    fn missing_directory_and_unicode_file_are_classified_without_rewriting_paths() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let unicode = temporary.path().join("Grüße 第一.txt");
        std::fs::write(&unicode, b"available").expect("unicode fixture");
        let mut accessibility = FileAttachmentAccessibility;
        assert_eq!(
            accessibility.check(&unicode),
            Ok(AttachmentAvailability::Available)
        );
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

    #[test]
    fn authoritative_ubiquitous_metadata_distinguishes_cloud_states() {
        let missing = tempfile::tempdir()
            .expect("temporary directory")
            .path()
            .join("evicted.txt");
        let in_cloud = FixedMetadata(Some(CloudMetadata {
            ubiquitous: Some(true),
            downloading: Some(false),
            status: Some(CloudDownloadStatus::NotDownloaded),
            has_download_error: false,
        }));
        assert_eq!(
            check_with_metadata(&missing, &in_cloud),
            Ok(AttachmentAvailability::InCloud)
        );

        for status in [
            CloudDownloadStatus::NotDownloaded,
            CloudDownloadStatus::Downloaded,
        ] {
            let downloading = FixedMetadata(Some(CloudMetadata {
                ubiquitous: Some(true),
                downloading: Some(true),
                status: Some(status),
                has_download_error: false,
            }));
            assert_eq!(
                check_with_metadata(&missing, &downloading),
                Ok(AttachmentAvailability::Downloading)
            );
        }
    }

    #[test]
    fn downloaded_and_current_metadata_still_require_exact_readability() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let readable = temporary.path().join("local.txt");
        std::fs::write(&readable, b"content").expect("readable fixture");
        for status in [
            CloudDownloadStatus::Downloaded,
            CloudDownloadStatus::Current,
        ] {
            let metadata = FixedMetadata(Some(CloudMetadata {
                ubiquitous: Some(true),
                downloading: Some(false),
                status: Some(status),
                has_download_error: false,
            }));
            assert_eq!(
                check_with_metadata(&readable, &metadata),
                Ok(AttachmentAvailability::Available)
            );
            assert_eq!(
                check_with_metadata(&temporary.path().join("missing.txt"), &metadata),
                Err(AttachmentAccessFailure::Missing)
            );
        }
    }

    #[test]
    fn unavailable_contradictory_or_failed_metadata_never_claims_icloud() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let missing = temporary.path().join("missing.txt");
        let cases = [
            None,
            Some(CloudMetadata {
                ubiquitous: Some(false),
                downloading: Some(true),
                status: Some(CloudDownloadStatus::NotDownloaded),
                has_download_error: false,
            }),
            Some(CloudMetadata {
                ubiquitous: Some(true),
                downloading: None,
                status: Some(CloudDownloadStatus::NotDownloaded),
                has_download_error: false,
            }),
            Some(CloudMetadata {
                ubiquitous: Some(true),
                downloading: Some(false),
                status: Some(CloudDownloadStatus::Unknown),
                has_download_error: false,
            }),
            Some(CloudMetadata {
                ubiquitous: Some(true),
                downloading: Some(true),
                status: Some(CloudDownloadStatus::Current),
                has_download_error: false,
            }),
            Some(CloudMetadata {
                ubiquitous: Some(true),
                downloading: Some(true),
                status: Some(CloudDownloadStatus::NotDownloaded),
                has_download_error: true,
            }),
        ];
        for metadata in cases {
            assert_eq!(
                check_with_metadata(&missing, &FixedMetadata(metadata)),
                Err(AttachmentAccessFailure::Missing)
            );
        }
    }
}
