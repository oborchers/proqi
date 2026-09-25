//! Filesystem boundary for plain-text thought export and destination completion.

use std::path::PathBuf;

use thiserror::Error;

/// Content-free identity of an existing file observed before a replacement decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExistingFile {
    /// Device identifier.
    pub device: u64,
    /// Inode number.
    pub inode: u64,
    /// Byte length.
    pub length: u64,
    /// Modification time in nanoseconds since the Unix epoch.
    pub modified_nanos: i128,
}

/// Whether an existing target may be replaced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportOverwrite {
    /// Never replace an existing entry.
    Refuse,
    /// Never replace, but accept a regular file that already holds exactly the exported bytes.
    RefuseUnlessIdentical,
    /// Replace only the exact file the user confirmed.
    Confirmed(ExistingFile),
    /// Replace any existing regular file, as requested by an explicit CLI flag.
    Always,
}

/// One atomic plain-text file write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportWriteRequest {
    /// Absolute destination path.
    pub path: PathBuf,
    /// Exact bytes to write.
    pub content: String,
    /// Replacement policy.
    pub overwrite: ExportOverwrite,
}

/// A durable export: the file and its directory entry were synchronized.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportWritten {
    /// Absolute destination path as written.
    pub path: PathBuf,
    /// Number of bytes in the file.
    pub bytes: u64,
    /// Whether an existing file was replaced.
    pub replaced: bool,
    /// Whether an existing file already held exactly these bytes and was left untouched.
    pub unchanged: bool,
}

/// Typed export write failure. Nothing was replaced unless stated otherwise.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ExportWriteError {
    /// The destination is not absolute or has no file name.
    #[error("the destination is not an absolute file path")]
    InvalidPath,
    /// A regular file already exists and replacement was not authorized.
    #[error("a file already exists at the destination")]
    Exists(ExistingFile),
    /// The parent directory does not exist. It is never created.
    #[error("the folder does not exist")]
    DirectoryMissing,
    /// A parent component is not a directory.
    #[error("a parent path is not a folder")]
    ParentNotDirectory,
    /// The destination is a directory.
    #[error("the destination is a folder")]
    TargetIsDirectory,
    /// The destination is a symbolic link, which export never replaces.
    #[error("the destination is a symbolic link; enter the path it points to")]
    TargetIsSymlink,
    /// The destination is neither a regular file nor absent.
    #[error("the destination is not a regular file")]
    TargetNotRegular,
    /// The confirmed file changed or disappeared before replacement.
    #[error("the file changed after confirmation; nothing was replaced")]
    Changed,
    /// The file system cannot exchange files atomically, so replacement is refused.
    #[error("this file system cannot replace a file safely; choose a new name")]
    ReplaceUnsupported,
    /// An unexpected entry could not be restored and was kept at this path.
    #[error("an unexpected entry could not be restored and was kept as {0}")]
    Displaced(String),
    /// The operating system denied access.
    #[error("permission denied")]
    PermissionDenied,
    /// The file system is read-only.
    #[error("the file system is read-only")]
    ReadOnly,
    /// No space or quota remained.
    #[error("the disk is full")]
    StorageFull,
    /// Another input or output failure.
    #[error("the file could not be written")]
    Io,
}

impl ExportWriteError {
    /// Stable machine reason used by the CLI and diagnostics.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::InvalidPath => "invalid_path",
            Self::Exists(_) => "exists",
            Self::DirectoryMissing => "directory_missing",
            Self::ParentNotDirectory => "parent_not_directory",
            Self::TargetIsDirectory => "target_is_directory",
            Self::TargetIsSymlink => "target_is_symlink",
            Self::TargetNotRegular => "target_not_regular",
            Self::Changed => "changed",
            Self::ReplaceUnsupported => "replace_unsupported",
            Self::Displaced(_) => "displaced",
            Self::PermissionDenied => "permission_denied",
            Self::ReadOnly => "read_only",
            Self::StorageFull => "storage_full",
            Self::Io => "io",
        }
    }
}

/// Atomically writes exported text, durable before it returns success.
pub trait ExportWriter {
    /// Write through a temporary file in the destination directory, synchronize it,
    /// persist it over or beside the target, and synchronize the directory.
    ///
    /// # Errors
    ///
    /// Returns a typed failure. A failure never leaves a partial destination file.
    fn write(&mut self, request: &ExportWriteRequest) -> Result<ExportWritten, ExportWriteError>;
}

/// One directory entry offered for destination completion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryEntry {
    /// UTF-8 entry name.
    pub name: String,
    /// Whether the entry is a directory, following symbolic links.
    pub directory: bool,
}

/// Bounded result of listing one directory.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DirectoryListing {
    /// Entries sorted by name.
    pub entries: Vec<DirectoryEntry>,
    /// Whether the listing stopped at the entry bound.
    pub truncated: bool,
}

/// Listing failure for completion. Completion simply offers nothing.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum DirectoryListingError {
    /// The directory does not exist or is not a directory.
    #[error("the folder does not exist")]
    Missing,
    /// The directory could not be read.
    #[error("the folder could not be read")]
    Unreadable,
}

/// Lists directory entries for destination completion.
pub trait DirectoryLister {
    /// List at most a bounded number of UTF-8 entries of one absolute directory.
    ///
    /// # Errors
    ///
    /// Returns a typed failure when the directory is absent or unreadable.
    fn list(
        &mut self,
        directory: &std::path::Path,
    ) -> Result<DirectoryListing, DirectoryListingError>;
}
