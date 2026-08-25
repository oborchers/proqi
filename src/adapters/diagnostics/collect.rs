//! Versioned local diagnostics bundle collection.

use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::Path,
};

use serde::Serialize;

use super::DiagnosticsError;

/// Versioned, content-redacted local support bundle.
#[derive(Debug, Serialize)]
pub struct DiagnosticBundle {
    /// Bundle format version.
    pub schema_version: u32,
    /// Proqi binary version.
    pub proqi_version: &'static str,
    /// Structured events grouped by instance segment.
    pub files: Vec<DiagnosticFile>,
}

/// One bounded JSONL segment represented without its machine path.
#[derive(Debug, Serialize)]
pub struct DiagnosticFile {
    /// Filename only.
    pub name: String,
    /// Valid structured event objects.
    pub events: Vec<serde_json::Value>,
}

/// Collect all retained JSONL segments and write one private JSON document.
///
/// # Errors
///
/// Returns a typed I/O or serialization error. Existing output files are never overwritten.
pub fn collect_bundle(
    data_dir: &Path,
    output: &Path,
) -> Result<DiagnosticBundle, DiagnosticsError> {
    let directory = data_dir.join("diagnostics");
    let mut files = Vec::new();
    if directory.is_dir() {
        let mut paths = fs::read_dir(&directory)?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .map(|entry| entry.path())
            .filter(|path| is_log(path))
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            files.push(read_file(&path)?);
        }
    }
    let bundle = DiagnosticBundle {
        schema_version: 1,
        proqi_version: env!("CARGO_PKG_VERSION"),
        files,
    };
    write_private_json(output, &bundle)?;
    Ok(bundle)
}

fn read_file(path: &Path) -> Result<DiagnosticFile, DiagnosticsError> {
    let content = fs::read_to_string(path)?;
    let events = content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("redacted.jsonl")
        .to_owned();
    Ok(DiagnosticFile { name, events })
}

fn write_private_json(output: &Path, bundle: &DiagnosticBundle) -> Result<(), DiagnosticsError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(output)?;
    serde_json::to_writer_pretty(&mut file, bundle)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    make_private(output)?;
    Ok(())
}

fn is_log(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let Some((stem, suffix)) = name.split_once(".jsonl") else {
                return false;
            };
            !stem.is_empty()
                && (suffix.is_empty()
                    || suffix
                        .strip_prefix('.')
                        .is_some_and(|index| index.parse::<usize>().is_ok()))
        })
}

fn make_private(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}
