//! macOS kqueue watcher with identity-based stable-file reconciliation.

use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs::{self, File},
    os::{macos::fs::MetadataExt as _, unix::fs::MetadataExt as _},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use kqueue::{EventFilter, FilterFlag, Watcher};
use rustix::fs::{Mode, OFlags, openat};
use sha2::{Digest as _, Sha256};
use xattr::FileExt as _;

use super::pattern::wildcard_match;
use crate::ports::screenshot::{
    MAX_RECONCILIATION_ENTRIES, ScreenshotCancellation, ScreenshotCandidate, ScreenshotError,
    ScreenshotFingerprint, ScreenshotImageType, ScreenshotInboxConfig,
};

const FINAL_EVENT_LIMIT: usize = 32;

trait DirectoryEvents: Send {
    fn wait(&mut self, timeout: Duration) -> Result<bool, ScreenshotError>;
}

trait MonotonicClock: Send + Sync {
    fn now(&self) -> Duration;
}

struct KqueueEvents(Watcher);

impl DirectoryEvents for KqueueEvents {
    fn wait(&mut self, timeout: Duration) -> Result<bool, ScreenshotError> {
        Ok(self.0.poll(Some(timeout)).is_some())
    }
}

struct SystemMonotonicClock(Instant);

impl MonotonicClock for SystemMonotonicClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}

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
    stable_since: Duration,
}

pub(super) struct MacScreenshotWatcher {
    config: ScreenshotInboxConfig,
    terminal_host: String,
    directory: File,
    events: Box<dyn DirectoryEvents>,
    clock: Arc<dyn MonotonicClock>,
    cancellation: Arc<dyn ScreenshotCancellation>,
    entry_limit: usize,
    baseline: HashSet<FileIdentity>,
    delivered: HashSet<FileIdentity>,
    pending: HashMap<FileIdentity, PendingCandidate>,
    next_observation: u64,
}

impl MacScreenshotWatcher {
    pub(super) fn start(
        config: ScreenshotInboxConfig,
        terminal_host: &str,
        cancellation: Arc<dyn ScreenshotCancellation>,
    ) -> Result<Self, ScreenshotError> {
        config.validate()?;
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
        Self::initialize(
            config,
            terminal_host,
            directory,
            Box::new(KqueueEvents(watcher)),
            Arc::new(SystemMonotonicClock(Instant::now())),
            cancellation,
            MAX_RECONCILIATION_ENTRIES,
            activation,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "watcher dependencies stay explicit"
    )]
    fn initialize(
        config: ScreenshotInboxConfig,
        terminal_host: &str,
        directory: File,
        events: Box<dyn DirectoryEvents>,
        clock: Arc<dyn MonotonicClock>,
        cancellation: Arc<dyn ScreenshotCancellation>,
        entry_limit: usize,
        activation: u128,
    ) -> Result<Self, ScreenshotError> {
        let mut state = Self {
            config,
            terminal_host: terminal_host_label(terminal_host),
            directory,
            events,
            clock,
            cancellation,
            entry_limit,
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
        let now = state.clock.now();
        for file in state.scan()? {
            state.observe(file, now);
        }
        Ok(state)
    }

    pub(super) fn poll(&mut self) -> Result<Vec<ScreenshotCandidate>, ScreenshotError> {
        let now = self.clock.now();
        if self.ready_at(now) {
            return self.reconcile(now);
        }
        let wait = self.wait_duration(now);
        let hinted = self.events.wait(wait)?;
        let observed_at = self.clock.now();
        if hinted || self.ready_at(observed_at) {
            self.reconcile(observed_at)
        } else {
            Ok(Vec::new())
        }
    }

    pub(super) fn final_reconcile(&mut self) -> Result<Vec<ScreenshotCandidate>, ScreenshotError> {
        let started = self.clock.now();
        let deadline = started.saturating_add(self.config.debounce);
        let mut ready = self.reconcile(started)?;
        for _ in 0..FINAL_EVENT_LIMIT {
            if self.pending.is_empty() {
                break;
            }
            let now = self.clock.now();
            if now >= deadline {
                break;
            }
            let wait = self.wait_duration(now).min(deadline.saturating_sub(now));
            let hinted = self.events.wait(wait)?;
            let observed_at = self.clock.now();
            if hinted || self.ready_at(observed_at) || observed_at >= deadline {
                ready.extend(self.reconcile(observed_at)?);
            }
        }
        Ok(ready)
    }

    fn wait_duration(&self, now: Duration) -> Duration {
        self.pending
            .values()
            .map(|candidate| {
                candidate
                    .stable_since
                    .saturating_add(self.config.debounce)
                    .saturating_sub(now)
            })
            .min()
            .unwrap_or(self.config.debounce)
            .min(self.config.debounce)
    }

    fn ready_at(&self, now: Duration) -> bool {
        self.pending
            .values()
            .any(|candidate| now.saturating_sub(candidate.stable_since) >= self.config.debounce)
    }

    fn reconcile(&mut self, now: Duration) -> Result<Vec<ScreenshotCandidate>, ScreenshotError> {
        let seen = self.scan()?;
        let identities = seen
            .iter()
            .map(|file| file.observation.identity)
            .collect::<HashSet<_>>();
        self.pending
            .retain(|identity, _| identities.contains(identity));
        for file in seen {
            self.observe(file, now);
        }
        let mut ready = self
            .pending
            .iter()
            .filter_map(|(identity, candidate)| {
                (now.saturating_sub(candidate.stable_since) >= self.config.debounce)
                    .then_some((*identity, candidate.first_observed))
            })
            .collect::<Vec<_>>();
        ready.sort_by_key(|(identity, order)| (*order, identity.birth_seconds, identity.inode));
        Ok(ready
            .into_iter()
            .filter_map(|(identity, _)| self.take_ready(identity))
            .collect())
    }

    fn observe(&mut self, file: ScannedFile, now: Duration) {
        let identity = file.observation.identity;
        if self.baseline.contains(&identity) || self.delivered.contains(&identity) {
            return;
        }
        if let Some(pending) = self.pending.get_mut(&identity) {
            if pending.observation != file.observation || pending.image_type != file.image_type {
                pending.observation = file.observation;
                pending.image_type = file.image_type;
                pending.stable_since = now;
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
                stable_since: now,
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
        for (index, entry) in entries.enumerate() {
            self.check_scan_bound(index)?;
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
        for (index, entry) in entries.enumerate() {
            self.check_scan_bound(index)?;
            let entry = entry.map_err(|_| ScreenshotError::Reconciliation)?;
            if let Some(identity) = self.inspect_identity(&entry.file_name())? {
                identities.push(identity);
            }
        }
        Ok(identities)
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
#[path = "macos/tests.rs"]
mod tests;
