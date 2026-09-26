//! Replacement permissions, vanished targets, identical retries, and injected failures.

use std::{
    fs,
    os::unix::fs::{MetadataExt as _, PermissionsExt as _},
};

use super::{leftovers, request};
use crate::ports::export::{ExistingFile, ExportOverwrite, ExportWriteError, ExportWriter as _};

fn mode(path: &std::path::Path) -> u32 {
    fs::metadata(path).expect("metadata").permissions().mode() & 0o7777
}

fn identity(path: &std::path::Path) -> ExistingFile {
    let metadata = fs::metadata(path).expect("metadata");
    ExistingFile {
        device: metadata.dev(),
        inode: metadata.ino(),
        length: metadata.len(),
        modified_nanos: i128::from(metadata.mtime()) * 1_000_000_000
            + i128::from(metadata.mtime_nsec()),
    }
}

#[test]
fn a_replacement_takes_the_replaced_mode_without_special_bits() {
    let temporary = tempfile::tempdir().expect("directory");
    let path = temporary.path().join("shared.txt");
    fs::write(&path, "old").expect("original");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o6754)).expect("special bits");
    super::FileExport::default()
        .write(&request(&path, "new", ExportOverwrite::Always))
        .expect("replace");
    assert_eq!(mode(&path), 0o754, "setuid and setgid are never copied");
    assert_eq!(fs::read_to_string(&path).expect("file"), "new");
    assert!(leftovers(temporary.path()).is_empty());

    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("readable");
    let confirmed = identity(&path);
    super::FileExport::default()
        .write(&request(
            &path,
            "newer",
            ExportOverwrite::Confirmed(confirmed),
        ))
        .expect("confirmed replace");
    assert_eq!(mode(&path), 0o644, "a confirmed replacement keeps 0644");
}

#[test]
fn replace_existing_creates_a_target_that_vanished_before_the_exchange() {
    let temporary = tempfile::tempdir().expect("directory");
    let path = temporary.path().join("gone.txt");
    let staged = super::super::temporary_file(temporary.path(), b"new", 0o600).expect("staged");
    assert_eq!(
        super::super::replace(staged, &request(&path, "new", ExportOverwrite::Always)),
        Ok(false),
        "created, not replaced"
    );
    assert_eq!(fs::read_to_string(&path).expect("file"), "new");
    assert!(leftovers(temporary.path()).is_empty());
}

#[test]
fn an_identical_retry_never_reads_or_waits_on_a_swapped_in_fifo() {
    let temporary = tempfile::tempdir().expect("directory");
    let path = temporary.path().join("notes.txt");
    fs::write(&path, "same").expect("original");
    let inspected = identity(&path);
    assert_eq!(
        super::super::holds_exactly(&path, b"same", inspected),
        Ok(true)
    );
    fs::remove_file(&path).expect("remove");
    // rustix offers no FIFO constructor on macOS, so the system tool makes one.
    let made = std::process::Command::new("mkfifo")
        .arg(&path)
        .status()
        .expect("mkfifo");
    assert!(made.success(), "fifo swapped in");
    assert_eq!(
        super::super::holds_exactly(&path, b"same", inspected),
        Ok(false),
        "the FIFO is opened without blocking and rejected by its handle"
    );
    let link = temporary.path().join("link.txt");
    std::os::unix::fs::symlink(&path, &link).expect("link");
    assert_eq!(
        super::super::holds_exactly(&link, b"same", inspected),
        Err(ExportWriteError::TargetIsSymlink)
    );
}

#[test]
fn an_injected_failure_comes_from_the_constructor_and_writes_nothing() {
    let temporary = tempfile::tempdir().expect("directory");
    let path = temporary.path().join("full.txt");
    let mut export = super::FileExport::with_injected_failure(Some(ExportWriteError::StorageFull));
    assert_eq!(
        export.write(&request(&path, "x", ExportOverwrite::Refuse)),
        Err(ExportWriteError::StorageFull)
    );
    assert!(!path.exists());
    assert!(leftovers(temporary.path()).is_empty());
}
