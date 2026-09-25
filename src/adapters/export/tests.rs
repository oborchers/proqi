use std::{
    fs,
    io::{self, ErrorKind},
    os::unix::fs::{PermissionsExt as _, symlink},
    path::{Path, PathBuf},
};

use super::{FileExport, MAX_LISTED_ENTRIES, map_io};
use crate::ports::export::{
    DirectoryLister as _, DirectoryListingError, ExportOverwrite, ExportWriteError,
    ExportWriteRequest, ExportWriter as _,
};

fn request(path: &Path, content: &str, overwrite: ExportOverwrite) -> ExportWriteRequest {
    ExportWriteRequest {
        path: path.to_path_buf(),
        content: content.to_owned(),
        overwrite,
    }
}

fn leftovers(directory: &Path) -> Vec<PathBuf> {
    fs::read_dir(directory)
        .expect("directory")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".proqi-export-"))
        })
        .collect()
}

#[test]
fn new_files_are_exact_durable_and_follow_the_umask() {
    let temporary = tempfile::tempdir().expect("directory");
    let path = temporary.path().join("Grüße 👩‍💻.txt");
    let content = "line one\r\n\n\ttabs  and spaces \u{0301}\n";
    let written = FileExport
        .write(&request(&path, content, ExportOverwrite::Refuse))
        .expect("write");
    assert_eq!(fs::read(&path).expect("read"), content.as_bytes());
    assert_eq!(written.bytes, content.len() as u64);
    assert!(!written.replaced && !written.unchanged);
    let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
    assert_eq!(mode & 0o600, 0o600, "owner can read and write");
    assert_eq!(mode & 0o111, 0, "export is never executable");
    assert!(leftovers(temporary.path()).is_empty());
}

#[test]
fn existing_files_are_never_replaced_without_authority() {
    let temporary = tempfile::tempdir().expect("directory");
    let path = temporary.path().join("notes.txt");
    fs::write(&path, "original").expect("original");
    let Err(ExportWriteError::Exists(existing)) =
        FileExport.write(&request(&path, "new", ExportOverwrite::Refuse))
    else {
        panic!("existing file must be reported");
    };
    assert_eq!(existing.length, 8);
    assert_eq!(fs::read_to_string(&path).expect("read"), "original");

    let replaced = FileExport
        .write(&request(&path, "new", ExportOverwrite::Confirmed(existing)))
        .expect("confirmed replacement");
    assert!(replaced.replaced);
    assert_eq!(fs::read_to_string(&path).expect("read"), "new");
    assert!(leftovers(temporary.path()).is_empty());
}

#[test]
fn a_file_changed_or_removed_after_confirmation_is_not_replaced() {
    let temporary = tempfile::tempdir().expect("directory");
    let path = temporary.path().join("notes.txt");
    fs::write(&path, "original").expect("original");
    let Err(ExportWriteError::Exists(existing)) =
        FileExport.write(&request(&path, "new", ExportOverwrite::Refuse))
    else {
        panic!("exists");
    };
    fs::write(&path, "someone else wrote a longer file").expect("concurrent edit");
    assert_eq!(
        FileExport.write(&request(&path, "new", ExportOverwrite::Confirmed(existing))),
        Err(ExportWriteError::Changed)
    );
    assert_eq!(
        fs::read_to_string(&path).expect("read"),
        "someone else wrote a longer file"
    );
    fs::remove_file(&path).expect("remove");
    assert_eq!(
        FileExport.write(&request(&path, "new", ExportOverwrite::Confirmed(existing))),
        Err(ExportWriteError::Changed)
    );
    assert!(!path.exists());
    assert!(leftovers(temporary.path()).is_empty());
}

#[test]
fn identical_retries_converge_and_always_replaces_regular_files() {
    let temporary = tempfile::tempdir().expect("directory");
    let path = temporary.path().join("notes.txt");
    fs::write(&path, "same").expect("original");
    let unchanged = FileExport
        .write(&request(
            &path,
            "same",
            ExportOverwrite::RefuseUnlessIdentical,
        ))
        .expect("identical");
    assert!(unchanged.unchanged && !unchanged.replaced);
    assert!(matches!(
        FileExport.write(&request(
            &path,
            "diff",
            ExportOverwrite::RefuseUnlessIdentical
        )),
        Err(ExportWriteError::Exists(_))
    ));
    let replaced = FileExport
        .write(&request(&path, "diff", ExportOverwrite::Always))
        .expect("always");
    assert!(replaced.replaced);
    assert_eq!(fs::read_to_string(&path).expect("read"), "diff");
}

#[test]
fn symlinks_directories_and_missing_parents_are_refused() {
    let temporary = tempfile::tempdir().expect("directory");
    let target = temporary.path().join("target.txt");
    fs::write(&target, "target").expect("target");
    let link = temporary.path().join("link.txt");
    symlink(&target, &link).expect("symlink");
    for overwrite in [ExportOverwrite::Refuse, ExportOverwrite::Always] {
        assert_eq!(
            FileExport.write(&request(&link, "x", overwrite)),
            Err(ExportWriteError::TargetIsSymlink)
        );
    }
    assert_eq!(fs::read_to_string(&target).expect("read"), "target");
    assert!(
        fs::symlink_metadata(&link)
            .expect("link")
            .file_type()
            .is_symlink()
    );

    assert_eq!(
        FileExport.write(&request(Path::new("/"), "x", ExportOverwrite::Always)),
        Err(ExportWriteError::InvalidPath)
    );
    let folder = temporary.path().join("folder");
    fs::create_dir(&folder).expect("folder");
    assert_eq!(
        FileExport.write(&request(&folder, "x", ExportOverwrite::Always)),
        Err(ExportWriteError::TargetIsDirectory)
    );
    let missing = temporary.path().join("missing/child.txt");
    assert_eq!(
        FileExport.write(&request(&missing, "x", ExportOverwrite::Refuse)),
        Err(ExportWriteError::DirectoryMissing)
    );
    assert!(!temporary.path().join("missing").exists(), "never created");
    let under_file = target.join("child.txt");
    assert_eq!(
        FileExport.write(&request(&under_file, "x", ExportOverwrite::Refuse)),
        Err(ExportWriteError::ParentNotDirectory)
    );
    assert_eq!(
        FileExport.write(&request(
            Path::new("relative.txt"),
            "x",
            ExportOverwrite::Refuse
        )),
        Err(ExportWriteError::InvalidPath)
    );
}

#[test]
fn read_only_directories_fail_without_partial_files() {
    let temporary = tempfile::tempdir().expect("directory");
    let locked = temporary.path().join("locked");
    fs::create_dir(&locked).expect("locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).expect("read only");
    let result = FileExport.write(&request(
        &locked.join("notes.txt"),
        "x",
        ExportOverwrite::Refuse,
    ));
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("restore");
    assert_eq!(result, Err(ExportWriteError::PermissionDenied));
    assert_eq!(fs::read_dir(&locked).expect("list").count(), 0);
}

#[test]
fn operating_system_failures_map_to_stable_reasons() {
    let cases = [
        (ErrorKind::StorageFull, ExportWriteError::StorageFull),
        (ErrorKind::QuotaExceeded, ExportWriteError::StorageFull),
        (ErrorKind::ReadOnlyFilesystem, ExportWriteError::ReadOnly),
        (
            ErrorKind::PermissionDenied,
            ExportWriteError::PermissionDenied,
        ),
        (ErrorKind::Interrupted, ExportWriteError::Io),
    ];
    for (kind, expected) in cases {
        assert_eq!(map_io(&io::Error::from(kind)), expected, "{kind:?}");
    }
    assert_eq!(ExportWriteError::StorageFull.reason(), "storage_full");
    assert_eq!(
        map_io(&io::Error::from_raw_os_error(28)),
        ExportWriteError::StorageFull
    );
}

#[test]
fn listings_are_sorted_utf8_and_mark_directories_through_links() {
    let temporary = tempfile::tempdir().expect("directory");
    fs::write(temporary.path().join("b.txt"), "").expect("file");
    fs::create_dir(temporary.path().join("a dir")).expect("dir");
    symlink(
        temporary.path().join("a dir"),
        temporary.path().join("c-link"),
    )
    .expect("link");
    let listing = FileExport.list(temporary.path()).expect("list");
    let names = listing
        .entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.directory))
        .collect::<Vec<_>>();
    assert_eq!(names, [("a dir", true), ("b.txt", false), ("c-link", true)]);
    assert!(!listing.truncated);
    assert_eq!(
        FileExport.list(&temporary.path().join("absent")),
        Err(DirectoryListingError::Missing)
    );
}

#[test]
fn listings_stop_at_their_bound() {
    let temporary = tempfile::tempdir().expect("directory");
    for index in 0..=MAX_LISTED_ENTRIES {
        fs::write(temporary.path().join(format!("{index:05}")), "").expect("file");
    }
    let listing = FileExport.list(temporary.path()).expect("list");
    assert_eq!(listing.entries.len(), MAX_LISTED_ENTRIES);
    assert!(listing.truncated);
}
