//! Cheap-first bounded directory inspection for the macOS watcher.

use std::{
    ffi::OsString,
    fs::{self, File},
    os::{macos::fs::MetadataExt as _, unix::fs::MetadataExt as _},
    path::PathBuf,
};

use rustix::fs::{Mode, OFlags, openat};
use xattr::FileExt as _;

use super::{
    FileIdentity, MacScreenshotWatcher, Observation, ScannedFile, access_error, permission_error,
};
use crate::{adapters::screenshot::pattern::wildcard_match, ports::screenshot::ScreenshotError};

const UF_HIDDEN: u32 = 0x0000_8000;

pub(super) struct CheapFile {
    pub(super) observation: Observation,
    pub(super) path: PathBuf,
    name: OsString,
    name_text: String,
}

impl MacScreenshotWatcher {
    pub(super) fn scan_cheap(&self) -> Result<Vec<CheapFile>, ScreenshotError> {
        let entries = fs::read_dir(&self.config.directory)
            .map_err(|error| access_error(&error, &self.terminal_host))?;
        let mut files = Vec::new();
        for (index, entry) in entries.enumerate() {
            self.check_scan_bound(index)?;
            let entry = entry.map_err(|_| ScreenshotError::Reconciliation)?;
            if let Some(file) = self.inspect_cheap(entry.file_name())? {
                files.push(file);
            }
        }
        files.sort_by_key(|file| {
            let identity = file.observation.identity;
            (identity.birth_seconds, identity.birth_nanos, identity.inode)
        });
        Ok(files)
    }

    pub(super) fn inspect_eligibility(
        &mut self,
        file: CheapFile,
    ) -> Result<Option<ScannedFile>, ScreenshotError> {
        if self.cancellation.is_cancelled() {
            return Err(ScreenshotError::Cancelled);
        }
        if file.observation.bytes < self.config.bounds.min_file_bytes
            || file.observation.bytes > self.config.bounds.max_file_bytes
        {
            return Ok(None);
        }
        let Some(opened) = self.open_regular(&file.name)? else {
            return Ok(None);
        };
        let metadata = opened
            .metadata()
            .map_err(|_| ScreenshotError::Reconciliation)?;
        if !metadata.is_file()
            || observation(&metadata) != file.observation
            || metadata.st_flags() & UF_HIDDEN != 0
        {
            return Ok(None);
        }
        #[cfg(test)]
        {
            self.eligibility_checks = self.eligibility_checks.saturating_add(1);
        }
        if !self.accepted_signal(&opened, &file.name_text) {
            return Ok(None);
        }
        let Some((image_type, _, _)) = super::super::image::inspect(&opened, self.config.bounds)
        else {
            return Ok(None);
        };
        if !self.config.supported_types.contains(&image_type) {
            return Ok(None);
        }
        Ok(Some(ScannedFile {
            observation: file.observation,
            path: file.path,
            image_type,
        }))
    }

    fn check_scan_bound(&self, index: usize) -> Result<(), ScreenshotError> {
        if self.cancellation.is_cancelled() {
            Err(ScreenshotError::Cancelled)
        } else if index >= self.entry_limit {
            Err(ScreenshotError::ReconciliationLimit)
        } else {
            Ok(())
        }
    }

    fn inspect_cheap(&self, name: OsString) -> Result<Option<CheapFile>, ScreenshotError> {
        let Some(name_text) = name.to_str() else {
            return Ok(None);
        };
        let name_text = name_text.to_owned();
        if name_text.starts_with('.') {
            return Ok(None);
        }
        let Some(file) = self.open_regular(&name)? else {
            return Ok(None);
        };
        let metadata = file
            .metadata()
            .map_err(|_| ScreenshotError::Reconciliation)?;
        if !metadata.is_file() || metadata.st_flags() & UF_HIDDEN != 0 {
            return Ok(None);
        }
        Ok(Some(CheapFile {
            observation: observation(&metadata),
            path: self.config.directory.join(&name),
            name,
            name_text,
        }))
    }

    fn open_regular(&self, name: &OsString) -> Result<Option<File>, ScreenshotError> {
        let descriptor = match openat(
            &self.directory,
            name,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        ) {
            Ok(descriptor) => descriptor,
            Err(rustix::io::Errno::LOOP | rustix::io::Errno::NOENT) => return Ok(None),
            Err(error)
                if error == rustix::io::Errno::ACCESS || error == rustix::io::Errno::PERM =>
            {
                return Err(permission_error(&self.terminal_host));
            }
            Err(_) => return Ok(None),
        };
        Ok(Some(File::from(descriptor)))
    }

    fn accepted_signal(&self, file: &File, name: &str) -> bool {
        file.get_xattr("com.apple.metadata:kMDItemIsScreenCapture")
            .is_ok_and(|value| value.is_some_and(|bytes| !bytes.is_empty()))
            || self
                .config
                .filename_patterns
                .iter()
                .any(|pattern| wildcard_match(pattern, name))
            || self.config.capture_all_new_images
    }
}

fn observation(metadata: &std::fs::Metadata) -> Observation {
    Observation {
        identity: FileIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
            birth_seconds: metadata.st_birthtime(),
            birth_nanos: metadata.st_birthtime_nsec(),
        },
        bytes: metadata.len(),
        modified_seconds: metadata.mtime(),
        modified_nanos: metadata.mtime_nsec(),
    }
}
