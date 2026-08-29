//! macOS kqueue watcher with identity-based stable-file reconciliation.

use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs::{self, File},
    os::{macos::fs::MetadataExt as _, unix::fs::MetadataExt as _},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use kqueue::{EventFilter, FilterFlag, Watcher};
use rustix::fs::{Mode, OFlags, openat};
use sha2::{Digest as _, Sha256};
use xattr::FileExt as _;

use super::pattern::wildcard_match;
use crate::ports::screenshot::{
    ScreenshotCandidate, ScreenshotError, ScreenshotFingerprint, ScreenshotImageType,
    ScreenshotInboxConfig,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    birth_seconds: i64,
    birth_nanos: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Observation {
    identity: FileIdentity,
    bytes: u64,
    modified_seconds: i64,
    modified_nanos: i64,
}

struct PendingCandidate {
    observation: Observation,
    path: PathBuf,
    image_type: ScreenshotImageType,
    first_observed: u64,
    stable_observations: u8,
}

pub(super) struct MacScreenshotWatcher {
    config: ScreenshotInboxConfig,
    terminal_host: String,
    directory: File,
    watcher: Watcher,
    baseline: HashSet<FileIdentity>,
    delivered: HashSet<FileIdentity>,
    pending: HashMap<FileIdentity, PendingCandidate>,
    next_observation: u64,
}

impl MacScreenshotWatcher {
    pub(super) fn start(
        config: ScreenshotInboxConfig,
        terminal_host: &str,
    ) -> Result<Self, ScreenshotError> {
        let directory = File::open(&config.directory)
            .map_err(|error| start_access_error(&error, terminal_host))?;
        let metadata = directory
            .metadata()
            .map_err(|error| access_error(&error, terminal_host))?;
        if !metadata.is_dir() {
            return Err(ScreenshotError::InvalidConfig(
                "screenshot inbox directory must exist and be a directory",
            ));
        }
        let mut watcher = Watcher::new().map_err(|_| ScreenshotError::Watcher)?;
        watcher
            .add_file(
                &directory,
                EventFilter::EVFILT_VNODE,
                FilterFlag::NOTE_WRITE
                    | FilterFlag::NOTE_EXTEND
                    | FilterFlag::NOTE_ATTRIB
                    | FilterFlag::NOTE_RENAME
                    | FilterFlag::NOTE_DELETE
                    | FilterFlag::NOTE_REVOKE,
            )
            .and_then(|()| watcher.watch())
            .map_err(|_| ScreenshotError::Watcher)?;
        let activation = system_nanos();
        let mut state = Self {
            config,
            terminal_host: terminal_host_label(terminal_host),
            directory,
            watcher,
            baseline: HashSet::new(),
            delivered: HashSet::new(),
            pending: HashMap::new(),
            next_observation: 0,
        };
        for identity in state.scan_identities()? {
            if birth_nanos(identity) <= activation {
                state.baseline.insert(identity);
            }
        }
        for file in state.scan()? {
            state.observe(file);
        }
        Ok(state)
    }

    pub(super) fn poll(&mut self) -> Result<Vec<ScreenshotCandidate>, ScreenshotError> {
        let _hint = self.watcher.poll(Some(self.config.debounce));
        self.reconcile()
    }

    pub(super) fn final_reconcile(&mut self) -> Result<Vec<ScreenshotCandidate>, ScreenshotError> {
        let mut ready = self.reconcile()?;
        let _hint = self.watcher.poll(Some(self.config.debounce));
        ready.extend(self.reconcile()?);
        Ok(ready)
    }

    fn reconcile(&mut self) -> Result<Vec<ScreenshotCandidate>, ScreenshotError> {
        let seen = self.scan()?;
        let identities = seen
            .iter()
            .map(|file| file.observation.identity)
            .collect::<HashSet<_>>();
        self.pending
            .retain(|identity, _| identities.contains(identity));
        for file in seen {
            self.observe(file);
        }
        let mut ready = self
            .pending
            .iter()
            .filter_map(|(identity, candidate)| {
                (candidate.stable_observations >= 2)
                    .then_some((*identity, candidate.first_observed))
            })
            .collect::<Vec<_>>();
        ready.sort_by_key(|(identity, order)| (*order, identity.birth_seconds, identity.inode));
        Ok(ready
            .into_iter()
            .filter_map(|(identity, _)| self.take_ready(identity))
            .collect())
    }

    fn observe(&mut self, file: ScannedFile) {
        let identity = file.observation.identity;
        if self.baseline.contains(&identity) || self.delivered.contains(&identity) {
            return;
        }
        if let Some(pending) = self.pending.get_mut(&identity) {
            if pending.observation == file.observation && pending.image_type == file.image_type {
                pending.stable_observations = pending.stable_observations.saturating_add(1);
            } else {
                pending.observation = file.observation;
                pending.image_type = file.image_type;
                pending.stable_observations = 1;
            }
            pending.path = file.path;
            return;
        }
        self.next_observation = self.next_observation.saturating_add(1);
        self.pending.insert(
            identity,
            PendingCandidate {
                observation: file.observation,
                path: file.path,
                image_type: file.image_type,
                first_observed: self.next_observation,
                stable_observations: 1,
            },
        );
    }

    fn take_ready(&mut self, identity: FileIdentity) -> Option<ScreenshotCandidate> {
        let pending = self.pending.remove(&identity)?;
        self.delivered.insert(identity);
        Some(ScreenshotCandidate {
            fingerprint: fingerprint(identity),
            path: pending.path,
            image_type: pending.image_type,
        })
    }

    fn scan(&self) -> Result<Vec<ScannedFile>, ScreenshotError> {
        let entries = fs::read_dir(&self.config.directory)
            .map_err(|error| access_error(&error, &self.terminal_host))?;
        let mut files = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|_| ScreenshotError::Reconciliation)?;
            if let Some(file) = self.inspect(entry.file_name())? {
                files.push(file);
            }
        }
        files.sort_by_key(|file| {
            let identity = file.observation.identity;
            (identity.birth_seconds, identity.birth_nanos, identity.inode)
        });
        Ok(files)
    }

    fn scan_identities(&self) -> Result<Vec<FileIdentity>, ScreenshotError> {
        let entries = fs::read_dir(&self.config.directory)
            .map_err(|error| access_error(&error, &self.terminal_host))?;
        let mut identities = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|_| ScreenshotError::Reconciliation)?;
            if let Some(identity) = self.inspect_identity(&entry.file_name())? {
                identities.push(identity);
            }
        }
        Ok(identities)
    }

    fn inspect_identity(&self, name: &OsString) -> Result<Option<FileIdentity>, ScreenshotError> {
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
        let file = File::from(descriptor);
        let metadata = file
            .metadata()
            .map_err(|_| ScreenshotError::Reconciliation)?;
        if !metadata.is_file() {
            return Ok(None);
        }
        Ok(Some(FileIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
            birth_seconds: metadata.st_birthtime(),
            birth_nanos: metadata.st_birthtime_nsec(),
        }))
    }

    fn inspect(&self, name: OsString) -> Result<Option<ScannedFile>, ScreenshotError> {
        let Some(name_text) = name.to_str() else {
            return Ok(None);
        };
        let descriptor = match openat(
            &self.directory,
            &name,
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
        let file = File::from(descriptor);
        let metadata = file
            .metadata()
            .map_err(|_| ScreenshotError::Reconciliation)?;
        if !metadata.is_file()
            || metadata.len() < self.config.bounds.min_file_bytes
            || metadata.len() > self.config.bounds.max_file_bytes
            || !self.accepted_signal(&file, name_text)
        {
            return Ok(None);
        }
        let Some((image_type, _, _)) = super::image::inspect(&file, self.config.bounds) else {
            return Ok(None);
        };
        if !self.config.supported_types.contains(&image_type) {
            return Ok(None);
        }
        let identity = FileIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
            birth_seconds: metadata.st_birthtime(),
            birth_nanos: metadata.st_birthtime_nsec(),
        };
        Ok(Some(ScannedFile {
            observation: Observation {
                identity,
                bytes: metadata.len(),
                modified_seconds: metadata.mtime(),
                modified_nanos: metadata.mtime_nsec(),
            },
            path: self.config.directory.join(name),
            image_type,
        }))
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

struct ScannedFile {
    observation: Observation,
    path: PathBuf,
    image_type: ScreenshotImageType,
}

fn fingerprint(identity: FileIdentity) -> ScreenshotFingerprint {
    let mut digest = Sha256::new();
    digest.update(b"proqi-screenshot-source-v1\0");
    digest.update(identity.device.to_be_bytes());
    digest.update(identity.inode.to_be_bytes());
    digest.update(identity.birth_seconds.to_be_bytes());
    digest.update(identity.birth_nanos.to_be_bytes());
    ScreenshotFingerprint(digest.finalize().into())
}

fn birth_nanos(identity: FileIdentity) -> u128 {
    u128::try_from(identity.birth_seconds)
        .unwrap_or(0)
        .saturating_mul(1_000_000_000)
        .saturating_add(u128::try_from(identity.birth_nanos).unwrap_or(0))
}

fn system_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

fn access_error(error: &std::io::Error, terminal_host: &str) -> ScreenshotError {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        permission_error(terminal_host)
    } else {
        ScreenshotError::Reconciliation
    }
}

fn start_access_error(error: &std::io::Error, terminal_host: &str) -> ScreenshotError {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => permission_error(terminal_host),
        std::io::ErrorKind::NotFound => ScreenshotError::InvalidConfig(
            "screenshot inbox directory must exist and be a directory",
        ),
        _ => ScreenshotError::Watcher,
    }
}

fn permission_error(terminal_host: &str) -> ScreenshotError {
    ScreenshotError::PermissionDenied {
        terminal_host: terminal_host_label(terminal_host),
    }
}

fn terminal_host_label(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 80 || trimmed.chars().any(char::is_control) {
        "your terminal host".to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::symlink, time::Duration};

    use super::MacScreenshotWatcher;
    use crate::adapters::screenshot::pattern::wildcard_match;
    use crate::ports::screenshot::{ScreenshotBounds, ScreenshotImageType, ScreenshotInboxConfig};

    #[test]
    fn fallback_patterns_support_localized_unicode_names() {
        assert!(wildcard_match(
            "Bildschirmfoto *.png",
            "Bildschirmfoto 你好.png"
        ));
        assert!(wildcard_match("capture-??.jpg", "CAPTURE-01.JPG"));
        assert!(!wildcard_match("Screenshot *.png", "ordinary.png"));
    }

    #[test]
    fn activation_ignores_every_existing_identity_and_delivers_new_unicode_file_once() {
        let directory = tempfile::tempdir().expect("temporary watched directory");
        let existing = directory.path().join("existing-partial.png");
        fs::write(&existing, b"partial").expect("existing partial");
        let mut watcher = MacScreenshotWatcher::start(config(directory.path()), "Test Terminal")
            .expect("watcher");
        fs::write(&existing, png_bytes(20, 10, 80)).expect("complete existing");
        mark_screenshot(&existing);
        let created = directory.path().join("Unicode capture 你好 🖼️.png");
        fs::write(&created, png_bytes(20, 10, 80)).expect("new screenshot");
        mark_screenshot(&created);

        assert!(watcher.poll().expect("first observation").is_empty());
        let accepted = watcher.poll().expect("stable observation");
        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].path, created);
        fs::write(&created, png_bytes(20, 10, 96)).expect("repeat modify hint");
        assert!(watcher.poll().expect("deduplicated modify").is_empty());
    }

    #[test]
    fn partial_write_and_rename_completion_require_stability() {
        let directory = tempfile::tempdir().expect("temporary watched directory");
        let staging = tempfile::tempdir().expect("temporary staging directory");
        let mut watcher = MacScreenshotWatcher::start(config(directory.path()), "Test Terminal")
            .expect("watcher");
        let partial = directory.path().join("partial.png");
        fs::write(&partial, b"short").expect("partial write");
        assert!(watcher.poll().expect("partial scan").is_empty());
        fs::write(&partial, png_bytes(30, 20, 80)).expect("complete write");
        mark_screenshot(&partial);
        assert!(watcher.poll().expect("first complete scan").is_empty());
        assert_eq!(watcher.poll().expect("stable complete scan").len(), 1);

        let staged = staging.path().join("renamed.png");
        fs::write(&staged, png_bytes(40, 20, 80)).expect("staged image");
        mark_screenshot(&staged);
        let renamed = directory.path().join("renamed.png");
        fs::rename(staged, &renamed).expect("rename completion");
        assert!(watcher.poll().expect("first rename scan").is_empty());
        let accepted = watcher.poll().expect("stable rename scan");
        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].path, renamed);
    }

    #[test]
    fn symlink_oversize_and_unmarked_images_are_ignored() {
        let directory = tempfile::tempdir().expect("temporary watched directory");
        let outside = tempfile::tempdir().expect("temporary outside directory");
        let mut config = config(directory.path());
        config.bounds.max_file_bytes = 100;
        let mut watcher = MacScreenshotWatcher::start(config, "Test Terminal").expect("watcher");
        let target = outside.path().join("target.png");
        fs::write(&target, png_bytes(10, 10, 80)).expect("target");
        mark_screenshot(&target);
        symlink(&target, directory.path().join("link.png")).expect("symlink");
        let oversized = directory.path().join("oversized.png");
        fs::write(&oversized, png_bytes(10, 10, 120)).expect("oversized");
        mark_screenshot(&oversized);
        fs::write(directory.path().join("ordinary.png"), png_bytes(10, 10, 80))
            .expect("ordinary image");

        assert!(watcher.poll().expect("first scan").is_empty());
        assert!(watcher.poll().expect("second scan").is_empty());
    }

    fn config(directory: &std::path::Path) -> ScreenshotInboxConfig {
        ScreenshotInboxConfig {
            directory: directory.to_path_buf(),
            filename_patterns: Vec::new(),
            capture_all_new_images: false,
            supported_types: vec![ScreenshotImageType::Png],
            bounds: ScreenshotBounds::default(),
            debounce: Duration::from_millis(100),
        }
    }

    fn png_bytes(width: u32, height: u32, len: usize) -> Vec<u8> {
        let mut bytes = vec![0; len.max(24)];
        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        bytes[12..16].copy_from_slice(b"IHDR");
        bytes[16..20].copy_from_slice(&width.to_be_bytes());
        bytes[20..24].copy_from_slice(&height.to_be_bytes());
        bytes
    }

    fn mark_screenshot(path: &std::path::Path) {
        xattr::set(
            path,
            "com.apple.metadata:kMDItemIsScreenCapture",
            b"bplist00\x09",
        )
        .expect("screenshot metadata");
    }
}
