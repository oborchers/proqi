//! Atomic, durable plain-text export files and bounded destination listings.

use std::{
    fs::{self, File, Metadata, Permissions},
    io::{self, Read as _, Write as _},
    os::unix::fs::{MetadataExt as _, PermissionsExt as _},
    path::Path,
};

use crate::ports::export::{
    DirectoryEntry, DirectoryLister, DirectoryListing, DirectoryListingError, ExistingFile,
    ExportOverwrite, ExportWriteError, ExportWriteRequest, ExportWriter, ExportWritten,
};

/// Maximum number of matching entries one completion listing returns.
pub const MAX_LISTED_ENTRIES: usize = 4_096;

/// Maximum number of directory entries one completion listing inspects.
pub const MAX_SCANNED_ENTRIES: usize = 262_144;

/// Permission bits a replacement copies from the replaced file. Set-user-ID,
/// set-group-ID, and sticky bits are never copied.
const COPIED_MODE_BITS: u32 = 0o777;

/// Filesystem-backed export writer and completion lister.
#[derive(Clone, Debug, Default)]
pub struct FileExport {
    injected_failure: Option<ExportWriteError>,
}

impl FileExport {
    /// A writer that fails every write with `failure` just before staging, for
    /// deterministic real-terminal qualification. The composition root decides.
    #[must_use]
    pub const fn with_injected_failure(failure: Option<ExportWriteError>) -> Self {
        Self {
            injected_failure: failure,
        }
    }
}

/// An existing regular file at the destination and its current permission bits.
#[derive(Clone, Copy, Debug)]
struct Target {
    identity: ExistingFile,
    mode: u32,
}

impl ExportWriter for FileExport {
    fn write(&mut self, request: &ExportWriteRequest) -> Result<ExportWritten, ExportWriteError> {
        let path = request.path.as_path();
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(ExportWriteError::InvalidPath);
        }
        let parent = path.parent().ok_or(ExportWriteError::InvalidPath)?;
        require_directory(parent)?;
        let existing = inspect_target(path)?;
        if let Some(unchanged) = authorize(request, existing, parent)? {
            return Ok(unchanged);
        }
        if let Some(failure) = &self.injected_failure {
            return Err(failure.clone());
        }
        let content = request.content.as_bytes();
        let replaced = if let Some(target) = existing {
            replace(stage_replacement(parent, content, target)?, request)?
        } else {
            persist_new(temporary_file(parent, content, 0o666)?, path)?;
            false
        };
        sync_directory(parent).map_err(|_| ExportWriteError::WrittenUnconfirmed)?;
        Ok(written(request, replaced, false))
    }
}

/// Stage a replacement that already carries the replaced file's permission bits,
/// so it never appears with other access than the file it replaces.
fn stage_replacement(
    parent: &Path,
    content: &[u8],
    target: Target,
) -> Result<tempfile::NamedTempFile, ExportWriteError> {
    let staged = temporary_file(parent, content, 0o600)?;
    staged
        .as_file()
        .set_permissions(Permissions::from_mode(target.mode & COPIED_MODE_BITS))
        .and_then(|()| staged.as_file().sync_all())
        .map_err(|error| map_io(&error))?;
    Ok(staged)
}

/// Decide whether an existing target may be replaced before any byte is written.
///
/// Returns a completed result when an identical file already satisfies the request.
fn authorize(
    request: &ExportWriteRequest,
    existing: Option<Target>,
    parent: &Path,
) -> Result<Option<ExportWritten>, ExportWriteError> {
    let Some(Target {
        identity: existing, ..
    }) = existing
    else {
        return match request.overwrite {
            ExportOverwrite::Confirmed(_) => Err(ExportWriteError::Changed),
            _ => Ok(None),
        };
    };
    match request.overwrite {
        ExportOverwrite::RefuseUnlessIdentical
            if holds_exactly(&request.path, request.content.as_bytes(), existing)? =>
        {
            // The identical file is already in place, so an unconfirmed flush is
            // reported exactly like one after a first write.
            synchronize(&request.path, parent).map_err(|_| ExportWriteError::WrittenUnconfirmed)?;
            Ok(Some(written(request, false, true)))
        }
        ExportOverwrite::Refuse | ExportOverwrite::RefuseUnlessIdentical => {
            Err(ExportWriteError::Exists(existing))
        }
        ExportOverwrite::Confirmed(expected) if expected != existing => {
            Err(ExportWriteError::Changed)
        }
        ExportOverwrite::Confirmed(_) | ExportOverwrite::Always => Ok(None),
    }
}

/// Atomically exchange the synchronized temporary file with an authorized target.
///
/// The exchange moves the displaced entry to the temporary name, where it is
/// verified: it must be a regular file and, after a confirmation, the exact file
/// the user confirmed. Anything else is exchanged back, so a file or link that
/// appeared after the last check is never overwritten. From the moment of the
/// exchange the temporary guard is disarmed, so no failure path can delete the
/// displaced entry. A file system without an atomic exchange refuses replacement
/// instead of racing a check against a rename. Under `Always`, a target that
/// disappeared before the exchange is simply created.
///
/// Returns whether an existing file was replaced.
fn replace(
    temporary: tempfile::NamedTempFile,
    request: &ExportWriteRequest,
) -> Result<bool, ExportWriteError> {
    let expected = match request.overwrite {
        ExportOverwrite::Confirmed(expected) => Some(expected),
        ExportOverwrite::Always => None,
        ExportOverwrite::Refuse | ExportOverwrite::RefuseUnlessIdentical => {
            return Err(ExportWriteError::Io);
        }
    };
    match exchange(temporary.path(), &request.path) {
        Ok(()) => {}
        Err(rustix::io::Errno::NOENT) if expected.is_none() => {
            persist_new(temporary, &request.path)?;
            return Ok(false);
        }
        Err(rustix::io::Errno::NOENT) => return Err(ExportWriteError::Changed),
        Err(rustix::io::Errno::NOSYS | rustix::io::Errno::INVAL | rustix::io::Errno::NOTSUP) => {
            return Err(ExportWriteError::ReplaceUnsupported);
        }
        Err(error) => return Err(map_io(&io::Error::from(error))),
    }
    let (file, staged) = match temporary.keep() {
        Ok(kept) => kept,
        Err(error) => return Err(map_io(&error.error)),
    };
    let permissions = match verify_displaced(&staged, expected) {
        Ok(permissions) => permissions,
        Err(error) => return restore(&file, &staged, &request.path, error).map(|()| false),
    };
    // The bits were copied before the exchange; only a concurrent change of the
    // replaced file's mode needs another copy.
    let unchanged_mode = file
        .metadata()
        .is_ok_and(|own| own.permissions().mode() & COPIED_MODE_BITS == permissions.mode());
    if !unchanged_mode {
        file.set_permissions(permissions)
            .and_then(|()| file.sync_all())
            .map_err(|_| ExportWriteError::WrittenUnconfirmed)?;
    }
    // The temporary name now holds the replaced file.
    fs::remove_file(&staged)
        .map_err(|_| ExportWriteError::Displaced(staged.to_string_lossy().into_owned()))?;
    Ok(true)
}

/// Exchange an unexpected entry back into place and discard only this export's own
/// file. Anything else found at the temporary name is kept and reported.
fn restore(
    file: &File,
    staged: &Path,
    destination: &Path,
    error: ExportWriteError,
) -> Result<(), ExportWriteError> {
    let displaced = || ExportWriteError::Displaced(staged.to_string_lossy().into_owned());
    if exchange(staged, destination).is_err() {
        return Err(displaced());
    }
    let own = file.metadata().map_err(|_| displaced())?;
    let returned = fs::symlink_metadata(staged).map_err(|_| displaced())?;
    if (own.dev(), own.ino()) != (returned.dev(), returned.ino()) {
        return Err(displaced());
    }
    fs::remove_file(staged).map_err(|_| displaced())?;
    Err(error)
}

/// Accept only the regular file the request authorized, and return its permissions
/// so the replacement keeps them, as editors do.
fn verify_displaced(
    staged: &Path,
    expected: Option<ExistingFile>,
) -> Result<Permissions, ExportWriteError> {
    let displaced = fs::symlink_metadata(staged).map_err(|error| map_io(&error))?;
    let unexpected = if expected.is_some() {
        ExportWriteError::Changed
    } else if displaced.file_type().is_symlink() {
        ExportWriteError::TargetIsSymlink
    } else if displaced.is_dir() {
        ExportWriteError::TargetIsDirectory
    } else {
        ExportWriteError::TargetNotRegular
    };
    if !displaced.is_file()
        || expected.is_some_and(|expected| expected != existing_file(&displaced))
    {
        return Err(unexpected);
    }
    Ok(Permissions::from_mode(
        displaced.permissions().mode() & COPIED_MODE_BITS,
    ))
}

fn exchange(left: &Path, right: &Path) -> Result<(), rustix::io::Errno> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        left,
        rustix::fs::CWD,
        right,
        rustix::fs::RenameFlags::EXCHANGE,
    )
}

/// Install the synchronized temporary file only if the target is still absent.
///
/// The temporary file was already created in the same folder, so a denial or an
/// unsupported operation here comes from the file system's lack of an atomic
/// no-replace rename (tempfile then falls back to a hard link), not from access.
fn persist_new(temporary: tempfile::NamedTempFile, path: &Path) -> Result<(), ExportWriteError> {
    let Err(error) = temporary.persist_noclobber(path) else {
        return Ok(());
    };
    match error.error.kind() {
        io::ErrorKind::AlreadyExists => Err(match inspect_target(path)? {
            Some(target) => ExportWriteError::Exists(target.identity),
            None => ExportWriteError::Io,
        }),
        io::ErrorKind::PermissionDenied | io::ErrorKind::Unsupported => {
            Err(ExportWriteError::InstallUnsupported)
        }
        _ => Err(map_io(&error.error)),
    }
}

impl DirectoryLister for FileExport {
    fn list(
        &mut self,
        directory: &Path,
        prefix: &str,
    ) -> Result<DirectoryListing, DirectoryListingError> {
        let entries = fs::read_dir(directory).map_err(|error| match error.kind() {
            io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => {
                DirectoryListingError::Missing
            }
            _ => DirectoryListingError::Unreadable,
        })?;
        let mut listing = DirectoryListing::default();
        for (scanned, entry) in entries.enumerate() {
            if listing.entries.len() == MAX_LISTED_ENTRIES || scanned == MAX_SCANNED_ENTRIES {
                listing.truncated = true;
                break;
            }
            let Ok(entry) = entry else {
                continue;
            };
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if !name.starts_with(prefix) {
                continue;
            }
            let directory = entry
                .file_type()
                .is_ok_and(|kind| kind.is_dir() || (kind.is_symlink() && entry.path().is_dir()));
            listing.entries.push(DirectoryEntry { name, directory });
        }
        listing
            .entries
            .sort_by(|left, right| left.name.cmp(&right.name));
        Ok(listing)
    }
}

fn written(request: &ExportWriteRequest, replaced: bool, unchanged: bool) -> ExportWritten {
    ExportWritten {
        path: request.path.clone(),
        bytes: u64::try_from(request.content.len()).unwrap_or(u64::MAX),
        replaced,
        unchanged,
    }
}

fn require_directory(parent: &Path) -> Result<(), ExportWriteError> {
    match fs::metadata(parent) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(ExportWriteError::ParentNotDirectory),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Err(ExportWriteError::DirectoryMissing)
        }
        Err(error) if error.kind() == io::ErrorKind::NotADirectory => {
            Err(ExportWriteError::ParentNotDirectory)
        }
        Err(error) => Err(map_io(&error)),
    }
}

fn inspect_target(path: &Path) -> Result<Option<Target>, ExportWriteError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(map_io(&error)),
        Ok(metadata) if metadata.file_type().is_symlink() => Err(ExportWriteError::TargetIsSymlink),
        Ok(metadata) if metadata.is_dir() => Err(ExportWriteError::TargetIsDirectory),
        Ok(metadata) if !metadata.is_file() => Err(ExportWriteError::TargetNotRegular),
        Ok(metadata) => Ok(Some(Target {
            identity: existing_file(&metadata),
            mode: metadata.permissions().mode(),
        })),
    }
}

fn existing_file(metadata: &Metadata) -> ExistingFile {
    ExistingFile {
        device: metadata.dev(),
        inode: metadata.ino(),
        length: metadata.len(),
        modified_nanos: i128::from(metadata.mtime())
            .saturating_mul(1_000_000_000)
            .saturating_add(i128::from(metadata.mtime_nsec())),
    }
}

/// Whether the inspected regular file holds exactly `content`.
///
/// The file is opened without following a link and without blocking, and the
/// open handle must still be the inspected regular file, so an entry swapped in
/// after inspection (a FIFO, device, or link) is never read or waited on.
fn holds_exactly(
    path: &Path,
    content: &[u8],
    inspected: ExistingFile,
) -> Result<bool, ExportWriteError> {
    use rustix::fs::{Mode, OFlags};
    let descriptor = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| match error {
        rustix::io::Errno::LOOP => ExportWriteError::TargetIsSymlink,
        error => map_io(&io::Error::from(error)),
    })?;
    let mut file = File::from(descriptor);
    let metadata = file.metadata().map_err(|error| map_io(&error))?;
    if !metadata.is_file()
        || (metadata.dev(), metadata.ino()) != (inspected.device, inspected.inode)
    {
        return Ok(false);
    }
    let length = metadata.len();
    if u64::try_from(content.len()).ok() != Some(length) {
        return Ok(false);
    }
    let mut existing = Vec::with_capacity(content.len());
    file.read_to_end(&mut existing)
        .map_err(|error| map_io(&error))?;
    Ok(existing == content)
}

fn synchronize(path: &Path, parent: &Path) -> Result<(), ExportWriteError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| map_io(&error))?;
    sync_directory(parent)
}

fn temporary_file(
    parent: &Path,
    content: &[u8],
    mode: u32,
) -> Result<tempfile::NamedTempFile, ExportWriteError> {
    let mut temporary = tempfile::Builder::new()
        .prefix(".proqi-export-")
        .suffix(".tmp")
        .permissions(Permissions::from_mode(mode))
        .tempfile_in(parent)
        .map_err(|error| map_io(&error))?;
    temporary
        .write_all(content)
        .and_then(|()| temporary.flush())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| map_io(&error))?;
    Ok(temporary)
}

fn sync_directory(path: &Path) -> Result<(), ExportWriteError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| map_io(&error))
}

fn map_io(error: &io::Error) -> ExportWriteError {
    match error.kind() {
        io::ErrorKind::PermissionDenied => ExportWriteError::PermissionDenied,
        io::ErrorKind::ReadOnlyFilesystem => ExportWriteError::ReadOnly,
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => ExportWriteError::StorageFull,
        io::ErrorKind::NotFound => ExportWriteError::DirectoryMissing,
        io::ErrorKind::NotADirectory => ExportWriteError::ParentNotDirectory,
        _ => ExportWriteError::Io,
    }
}

#[cfg(test)]
#[path = "export/tests.rs"]
mod tests;
