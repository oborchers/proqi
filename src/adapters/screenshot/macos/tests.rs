use std::{
    collections::VecDeque,
    fs::{self, File},
    os::unix::fs::symlink,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use super::{DirectoryEvents, MacScreenshotWatcher, MonotonicClock, system_nanos};
use crate::{
    adapters::screenshot::pattern::wildcard_match,
    ports::screenshot::{
        ScreenshotBounds, ScreenshotCancellation, ScreenshotError, ScreenshotImageType,
        ScreenshotInboxConfig,
    },
};

#[derive(Default)]
struct FakeClock(AtomicU64);

impl FakeClock {
    fn advance(&self, duration: Duration) {
        self.0.fetch_add(
            u64::try_from(duration.as_millis()).expect("fake duration"),
            Ordering::Relaxed,
        );
    }
}

impl MonotonicClock for FakeClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.0.load(Ordering::Relaxed))
    }
}

struct FakeEvents {
    clock: Arc<FakeClock>,
    steps: Arc<Mutex<VecDeque<(Duration, bool)>>>,
}

#[derive(Clone)]
struct EventControl(Arc<Mutex<VecDeque<(Duration, bool)>>>);

impl EventControl {
    fn push(&self, elapsed: Duration, hinted: bool) {
        self.0
            .lock()
            .expect("event queue")
            .push_back((elapsed, hinted));
    }
}

impl DirectoryEvents for FakeEvents {
    fn wait(&mut self, timeout: Duration) -> Result<bool, ScreenshotError> {
        let (elapsed, hinted) = self
            .steps
            .lock()
            .expect("event queue")
            .pop_front()
            .unwrap_or((timeout, false));
        self.clock.advance(elapsed.min(timeout));
        Ok(hinted && elapsed <= timeout)
    }
}

#[derive(Default)]
struct TestCancellation {
    armed: AtomicBool,
    remaining: AtomicUsize,
}

impl TestCancellation {
    fn cancel_after_checks(&self, checks: usize) {
        self.remaining.store(checks, Ordering::Relaxed);
        self.armed.store(true, Ordering::Relaxed);
    }
}

impl ScreenshotCancellation for TestCancellation {
    fn is_cancelled(&self) -> bool {
        self.armed.load(Ordering::Relaxed)
            && self
                .remaining
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                    remaining.checked_sub(1)
                })
                .is_err()
    }
}

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
fn production_kqueue_starts_on_an_isolated_directory() {
    let directory = tempfile::tempdir().expect("watched directory");
    MacScreenshotWatcher::start(
        config(directory.path()),
        "Test Terminal",
        Arc::new(TestCancellation::default()),
    )
    .expect("production watcher");
}

#[test]
fn idle_poll_does_not_scan_without_a_hint() {
    let directory = tempfile::tempdir().expect("watched directory");
    let (mut watcher, events, _, _) = watcher(directory.path(), 1);
    for name in ["one", "two"] {
        fs::write(directory.path().join(name), b"entry").expect("entry");
    }

    assert!(watcher.poll().expect("idle poll").is_empty());
    events.push(Duration::ZERO, true);
    assert_eq!(
        watcher.poll().expect_err("hinted bounded scan"),
        ScreenshotError::ReconciliationLimit
    );
}

#[test]
fn rapid_equal_observations_and_later_mutation_require_a_full_interval() {
    let directory = tempfile::tempdir().expect("watched directory");
    let (mut watcher, events, clock, _) = watcher(directory.path(), 20);
    let path = directory.path().join("Unicode capture 你好 🖼️.png");
    fs::write(&path, png_bytes(20, 10, 80)).expect("new screenshot");
    mark_screenshot(&path);

    events.push(Duration::ZERO, true);
    assert!(watcher.poll().expect("first observation").is_empty());
    events.push(Duration::ZERO, true);
    assert!(watcher.poll().expect("rapid equal observation").is_empty());
    clock.advance(Duration::from_millis(90));
    fs::write(&path, png_bytes(20, 10, 96)).expect("later mutation");
    events.push(Duration::ZERO, true);
    assert!(watcher.poll().expect("mutation observation").is_empty());
    clock.advance(Duration::from_millis(99));
    events.push(Duration::ZERO, true);
    assert!(watcher.poll().expect("not stable long enough").is_empty());

    let accepted = watcher.poll().expect("full stable interval");
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].path, path);
}

#[test]
fn activation_ignores_existing_files_and_rename_completion_is_stable() {
    let directory = tempfile::tempdir().expect("watched directory");
    let staging = tempfile::tempdir().expect("staging directory");
    let existing = directory.path().join("existing.png");
    fs::write(&existing, b"partial").expect("existing partial");
    let (mut watcher, events, _, _) = watcher(directory.path(), 20);
    fs::write(&existing, png_bytes(20, 10, 80)).expect("existing completed");
    mark_screenshot(&existing);
    let staged = staging.path().join("staged.png");
    fs::write(&staged, png_bytes(30, 20, 80)).expect("staged image");
    mark_screenshot(&staged);
    let renamed = directory.path().join("renamed.png");
    fs::rename(staged, &renamed).expect("rename completion");

    events.push(Duration::ZERO, true);
    assert!(watcher.poll().expect("rename observation").is_empty());
    let accepted = watcher.poll().expect("stable rename");
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].path, renamed);
}

#[test]
fn symlink_oversize_and_unmarked_images_are_ignored() {
    let directory = tempfile::tempdir().expect("watched directory");
    let outside = tempfile::tempdir().expect("outside directory");
    let (mut watcher, events, _, _) = watcher_with_config(
        bounded_config(directory.path(), 100),
        20,
        Arc::new(TestCancellation::default()),
    );
    let target = outside.path().join("target.png");
    fs::write(&target, png_bytes(10, 10, 80)).expect("target");
    mark_screenshot(&target);
    symlink(&target, directory.path().join("link.png")).expect("symlink");
    let oversized = directory.path().join("oversized.png");
    fs::write(&oversized, png_bytes(10, 10, 120)).expect("oversized");
    mark_screenshot(&oversized);
    fs::write(directory.path().join("ordinary.png"), png_bytes(10, 10, 80))
        .expect("ordinary image");

    events.push(Duration::ZERO, true);
    assert!(watcher.poll().expect("bounded scan").is_empty());
    assert!(watcher.poll().expect("follow-up").is_empty());
}

#[test]
fn reconciliation_accepts_the_entry_limit_and_rejects_one_more() {
    let exact = tempfile::tempdir().expect("exact directory");
    for index in 0..3 {
        fs::write(exact.path().join(format!("entry-{index}")), b"entry").expect("entry");
    }
    watcher(exact.path(), 3);

    let over = tempfile::tempdir().expect("over directory");
    for index in 0..4 {
        fs::write(over.path().join(format!("entry-{index}")), b"entry").expect("entry");
    }
    assert!(matches!(
        try_watcher(over.path(), 3, Arc::new(TestCancellation::default())),
        Err(ScreenshotError::ReconciliationLimit)
    ));
}

#[test]
fn cancellation_interrupts_a_large_reconciliation_within_the_entry_bound() {
    let directory = tempfile::tempdir().expect("watched directory");
    let cancellation = Arc::new(TestCancellation::default());
    let (mut watcher, events, _, _) =
        watcher_with_config(config(directory.path()), 100, cancellation.clone());
    for index in 0..20 {
        fs::write(
            directory.path().join(format!("capture-{index}.png")),
            png_bytes(10, 10, 80),
        )
        .expect("image");
    }
    cancellation.cancel_after_checks(2);
    events.push(Duration::ZERO, true);
    assert_eq!(
        watcher.poll().expect_err("cancelled scan"),
        ScreenshotError::Cancelled
    );
}

fn watcher(
    directory: &std::path::Path,
    entry_limit: usize,
) -> (
    MacScreenshotWatcher,
    EventControl,
    Arc<FakeClock>,
    Arc<TestCancellation>,
) {
    watcher_with_config(
        config(directory),
        entry_limit,
        Arc::new(TestCancellation::default()),
    )
}

fn watcher_with_config(
    config: ScreenshotInboxConfig,
    entry_limit: usize,
    cancellation: Arc<TestCancellation>,
) -> (
    MacScreenshotWatcher,
    EventControl,
    Arc<FakeClock>,
    Arc<TestCancellation>,
) {
    let clock = Arc::new(FakeClock::default());
    let steps = Arc::new(Mutex::new(VecDeque::new()));
    let events = EventControl(steps.clone());
    let directory = File::open(&config.directory).expect("open watched directory");
    let watcher = MacScreenshotWatcher::initialize(
        config,
        "Test Terminal",
        directory,
        Box::new(FakeEvents {
            clock: clock.clone(),
            steps,
        }),
        clock.clone(),
        cancellation.clone(),
        entry_limit,
        system_nanos(),
    )
    .expect("watcher");
    (watcher, events, clock, cancellation)
}

fn try_watcher(
    directory: &std::path::Path,
    entry_limit: usize,
    cancellation: Arc<TestCancellation>,
) -> Result<MacScreenshotWatcher, ScreenshotError> {
    let clock = Arc::new(FakeClock::default());
    MacScreenshotWatcher::initialize(
        config(directory),
        "Test Terminal",
        File::open(directory).expect("open watched directory"),
        Box::new(FakeEvents {
            clock: clock.clone(),
            steps: Arc::new(Mutex::new(VecDeque::new())),
        }),
        clock,
        cancellation,
        entry_limit,
        system_nanos(),
    )
}

fn config(directory: &std::path::Path) -> ScreenshotInboxConfig {
    bounded_config(directory, ScreenshotBounds::default().max_file_bytes)
}

fn bounded_config(directory: &std::path::Path, max_file_bytes: u64) -> ScreenshotInboxConfig {
    ScreenshotInboxConfig {
        directory: directory.to_path_buf(),
        filename_patterns: Vec::new(),
        capture_all_new_images: false,
        supported_types: vec![ScreenshotImageType::Png],
        bounds: ScreenshotBounds {
            max_file_bytes,
            ..ScreenshotBounds::default()
        },
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
