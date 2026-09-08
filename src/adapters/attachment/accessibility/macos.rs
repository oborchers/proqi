//! Safe Foundation URL-resource metadata probe for macOS ubiquitous items.

use std::path::Path;

use objc2::runtime::AnyObject;
use objc2_foundation::{
    NSArray, NSDictionary, NSNumber, NSString, NSURL, NSURLResourceKey, ns_string,
};

use crate::ports::attachment_accessibility::AttachmentAvailability;

const STATUS_NOT_DOWNLOADED: &str = "NSURLUbiquitousItemDownloadingStatusNotDownloaded";
const STATUS_DOWNLOADED: &str = "NSURLUbiquitousItemDownloadingStatusDownloaded";
const STATUS_CURRENT: &str = "NSURLUbiquitousItemDownloadingStatusCurrent";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DownloadStatus {
    NotDownloaded,
    Downloaded,
    Current,
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Metadata {
    ubiquitous: Option<bool>,
    downloading: Option<bool>,
    status: Option<DownloadStatus>,
    has_download_error: bool,
}

trait MetadataProbe {
    fn metadata(&self, path: &Path) -> Option<Metadata>;
}

struct FoundationMetadata;

impl MetadataProbe for FoundationMetadata {
    fn metadata(&self, path: &Path) -> Option<Metadata> {
        let url = NSURL::from_file_path(path)?;
        let ubiquitous_key = ns_string!("NSURLIsUbiquitousItemKey");
        let downloading_key = ns_string!("NSURLUbiquitousItemIsDownloadingKey");
        let status_key = ns_string!("NSURLUbiquitousItemDownloadingStatusKey");
        let error_key = ns_string!("NSURLUbiquitousItemDownloadingErrorKey");
        let keys = NSArray::from_slice(&[ubiquitous_key, downloading_key, status_key, error_key]);
        let values = url.resourceValuesForKeys_error(&keys).ok()?;
        Some(Metadata {
            ubiquitous: bool_value(&values, ubiquitous_key),
            downloading: bool_value(&values, downloading_key),
            status: status_value(&values, status_key),
            has_download_error: values.objectForKey(error_key).is_some(),
        })
    }
}

pub(super) fn availability(path: &Path) -> Option<AttachmentAvailability> {
    availability_with_metadata(path, &FoundationMetadata)
}

fn availability_with_metadata(
    path: &Path,
    metadata: &impl MetadataProbe,
) -> Option<AttachmentAvailability> {
    classify(metadata.metadata(path)?)
}

fn classify(metadata: Metadata) -> Option<AttachmentAvailability> {
    if metadata.ubiquitous != Some(true) || metadata.has_download_error {
        return None;
    }
    match (metadata.downloading, metadata.status) {
        (Some(true), Some(DownloadStatus::NotDownloaded | DownloadStatus::Downloaded)) => {
            Some(AttachmentAvailability::Downloading)
        }
        (Some(false), Some(DownloadStatus::NotDownloaded)) => Some(AttachmentAvailability::InCloud),
        _ => None,
    }
}

fn bool_value(
    values: &NSDictionary<NSURLResourceKey, AnyObject>,
    key: &NSURLResourceKey,
) -> Option<bool> {
    values
        .objectForKey(key)?
        .downcast::<NSNumber>()
        .ok()
        .map(|value| value.as_bool())
}

fn status_value(
    values: &NSDictionary<NSURLResourceKey, AnyObject>,
    key: &NSURLResourceKey,
) -> Option<DownloadStatus> {
    let value = values.objectForKey(key)?.downcast::<NSString>().ok()?;
    Some(match value.to_string().as_str() {
        STATUS_NOT_DOWNLOADED => DownloadStatus::NotDownloaded,
        STATUS_DOWNLOADED => DownloadStatus::Downloaded,
        STATUS_CURRENT => DownloadStatus::Current,
        _ => DownloadStatus::Unknown,
    })
}

#[cfg(test)]
mod tests {
    use crate::ports::attachment_accessibility::{AttachmentAccessFailure, AttachmentAvailability};

    use super::super::finish_with_platform_availability;
    use super::{DownloadStatus, Metadata, MetadataProbe, availability_with_metadata};

    struct FixedMetadata(Option<Metadata>);

    impl MetadataProbe for FixedMetadata {
        fn metadata(&self, _path: &std::path::Path) -> Option<Metadata> {
            self.0
        }
    }

    #[test]
    fn authoritative_ubiquitous_metadata_distinguishes_cloud_states() {
        let path = std::path::Path::new("/private/evicted.txt");
        let in_cloud = FixedMetadata(Some(Metadata {
            ubiquitous: Some(true),
            downloading: Some(false),
            status: Some(DownloadStatus::NotDownloaded),
            has_download_error: false,
        }));
        assert_eq!(
            availability_with_metadata(path, &in_cloud),
            Some(AttachmentAvailability::InCloud)
        );

        for status in [DownloadStatus::NotDownloaded, DownloadStatus::Downloaded] {
            let downloading = FixedMetadata(Some(Metadata {
                ubiquitous: Some(true),
                downloading: Some(true),
                status: Some(status),
                has_download_error: false,
            }));
            assert_eq!(
                availability_with_metadata(path, &downloading),
                Some(AttachmentAvailability::Downloading)
            );
        }
    }

    #[test]
    fn downloaded_and_current_metadata_fall_through_to_exact_readability() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let readable = temporary.path().join("local.txt");
        let missing = temporary.path().join("missing.txt");
        std::fs::write(&readable, b"content").expect("readable fixture");
        for status in [DownloadStatus::Downloaded, DownloadStatus::Current] {
            let metadata = FixedMetadata(Some(Metadata {
                ubiquitous: Some(true),
                downloading: Some(false),
                status: Some(status),
                has_download_error: false,
            }));
            assert_eq!(availability_with_metadata(&readable, &metadata), None);
            assert_eq!(
                finish_with_platform_availability(&readable, None),
                Ok(AttachmentAvailability::Available)
            );
            assert_eq!(
                finish_with_platform_availability(&missing, None),
                Err(AttachmentAccessFailure::Missing)
            );
        }
    }

    #[test]
    fn unavailable_contradictory_or_failed_metadata_never_claims_icloud() {
        let path = std::path::Path::new("/private/missing.txt");
        let cases = [
            None,
            Some(Metadata {
                ubiquitous: Some(false),
                downloading: Some(true),
                status: Some(DownloadStatus::NotDownloaded),
                has_download_error: false,
            }),
            Some(Metadata {
                ubiquitous: Some(true),
                downloading: None,
                status: Some(DownloadStatus::NotDownloaded),
                has_download_error: false,
            }),
            Some(Metadata {
                ubiquitous: Some(true),
                downloading: Some(false),
                status: Some(DownloadStatus::Unknown),
                has_download_error: false,
            }),
            Some(Metadata {
                ubiquitous: Some(true),
                downloading: Some(true),
                status: Some(DownloadStatus::Current),
                has_download_error: false,
            }),
            Some(Metadata {
                ubiquitous: Some(true),
                downloading: Some(true),
                status: Some(DownloadStatus::NotDownloaded),
                has_download_error: true,
            }),
        ];
        for metadata in cases {
            assert_eq!(
                availability_with_metadata(path, &FixedMetadata(metadata)),
                None
            );
        }
    }

    #[test]
    fn metadata_probe_has_no_materialization_selector() {
        let source = include_str!("macos.rs");
        let materialization_selector = ["startDownloading", "UbiquitousItem"].concat();
        assert!(!source.contains(&materialization_selector));
    }
}
