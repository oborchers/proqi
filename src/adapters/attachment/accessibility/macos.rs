//! Safe Foundation URL-resource metadata probe for macOS ubiquitous items.

use std::path::Path;

use objc2::runtime::AnyObject;
use objc2_foundation::{
    NSArray, NSDictionary, NSNumber, NSString, NSURL, NSURLResourceKey, ns_string,
};

use super::{CloudDownloadStatus, CloudMetadata, CloudMetadataProbe};

const STATUS_NOT_DOWNLOADED: &str = "NSURLUbiquitousItemDownloadingStatusNotDownloaded";
const STATUS_DOWNLOADED: &str = "NSURLUbiquitousItemDownloadingStatusDownloaded";
const STATUS_CURRENT: &str = "NSURLUbiquitousItemDownloadingStatusCurrent";

pub(super) struct PlatformCloudMetadata;

impl CloudMetadataProbe for PlatformCloudMetadata {
    fn metadata(&self, path: &Path) -> Option<CloudMetadata> {
        let url = NSURL::from_file_path(path)?;
        let ubiquitous_key = ns_string!("NSURLIsUbiquitousItemKey");
        let downloading_key = ns_string!("NSURLUbiquitousItemIsDownloadingKey");
        let status_key = ns_string!("NSURLUbiquitousItemDownloadingStatusKey");
        let error_key = ns_string!("NSURLUbiquitousItemDownloadingErrorKey");
        let keys = NSArray::from_slice(&[ubiquitous_key, downloading_key, status_key, error_key]);
        let values = url.resourceValuesForKeys_error(&keys).ok()?;
        Some(CloudMetadata {
            ubiquitous: bool_value(&values, ubiquitous_key),
            downloading: bool_value(&values, downloading_key),
            status: status_value(&values, status_key),
            has_download_error: values.objectForKey(error_key).is_some(),
        })
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
) -> Option<CloudDownloadStatus> {
    let value = values.objectForKey(key)?.downcast::<NSString>().ok()?;
    Some(match value.to_string().as_str() {
        STATUS_NOT_DOWNLOADED => CloudDownloadStatus::NotDownloaded,
        STATUS_DOWNLOADED => CloudDownloadStatus::Downloaded,
        STATUS_CURRENT => CloudDownloadStatus::Current,
        _ => CloudDownloadStatus::Unknown,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn metadata_probe_has_no_materialization_selector() {
        let source = include_str!("macos.rs");
        let materialization_selector = ["startDownloading", "UbiquitousItem"].concat();
        assert!(!source.contains(&materialization_selector));
    }
}
