//! Read-only clipboard backed by a qualification fixture file.
//!
//! Process and live Herdr tests must never read or overwrite the user's real
//! clipboard, and the native clipboard is shared by every process of the user.
//! Only `capture_clipboard` selects this adapter, when its dedicated test
//! variable names a fixture; the adapter itself reads no environment.

use std::path::PathBuf;

use serde::Deserialize;

use crate::ports::{
    attachment::RasterImage,
    clipboard::{Clipboard, ClipboardContent, ClipboardError, ClipboardText, ClipboardWrite},
};

const MAX_FIXTURE_BYTES: u64 = 1024 * 1024;

/// Clipboard content described by one strict JSON document.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Fixture {
    /// Exact text.
    Text { text: String },
    /// A one-pixel image, standing in for any non-text content.
    Image,
    /// A clipboard that cannot be read.
    Unavailable,
}

/// Clipboard whose every read returns the fixture's current content.
#[derive(Clone, Debug)]
pub struct FixtureClipboard {
    path: PathBuf,
}

impl FixtureClipboard {
    /// Read the fixture at `path` on every clipboard read.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn fixture(&self) -> Result<Fixture, ClipboardError> {
        let unavailable = |reason: String| ClipboardError::Unavailable(reason);
        let file = std::fs::File::open(&self.path)
            .map_err(|error| unavailable(format!("clipboard fixture: {error}")))?;
        let mut contents = String::new();
        std::io::Read::read_to_string(
            &mut std::io::Read::take(file, MAX_FIXTURE_BYTES),
            &mut contents,
        )
        .map_err(|error| unavailable(format!("clipboard fixture: {error}")))?;
        serde_json::from_str(&contents)
            .map_err(|error| unavailable(format!("clipboard fixture: {error}")))
    }
}

impl Clipboard for FixtureClipboard {
    fn write(
        &mut self,
        _request_id: crate::domain::RequestId,
        _content: &ClipboardText,
    ) -> Result<ClipboardWrite, ClipboardError> {
        Err(ClipboardError::Unavailable(
            "the clipboard fixture is read-only".to_owned(),
        ))
    }

    fn read(&mut self) -> Result<ClipboardContent, ClipboardError> {
        match self.fixture()? {
            Fixture::Text { text } => Ok(ClipboardContent::Text(ClipboardText::plain(text))),
            Fixture::Image => RasterImage::new(1, 1, vec![0; 4])
                .map(ClipboardContent::Image)
                .map_err(|_| ClipboardError::InvalidImage),
            Fixture::Unavailable => Err(ClipboardError::Unavailable(
                "the clipboard fixture is unavailable".to_owned(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FixtureClipboard;
    use crate::ports::clipboard::{Clipboard as _, ClipboardContent, ClipboardError};

    fn read(fixture: &str) -> Result<ClipboardContent, ClipboardError> {
        let directory = tempfile::tempdir().expect("directory");
        let path = directory.path().join("clipboard.json");
        std::fs::write(&path, fixture).expect("fixture");
        FixtureClipboard::new(path).read()
    }

    #[test]
    fn fixture_kinds_map_to_exact_clipboard_results() {
        let Ok(ClipboardContent::Text(text)) = read(r#"{"kind":"text","text":" a\r\nb "}"#) else {
            panic!("text fixture");
        };
        assert_eq!(text.content(), " a\r\nb ");
        assert!(matches!(
            read(r#"{"kind":"image"}"#),
            Ok(ClipboardContent::Image(_))
        ));
        assert!(matches!(
            read(r#"{"kind":"unavailable"}"#),
            Err(ClipboardError::Unavailable(_))
        ));
    }

    #[test]
    fn malformed_or_missing_fixtures_are_unavailable_and_never_real() {
        assert!(matches!(
            read(r#"{"kind":"text","text":"x","extra":1}"#),
            Err(ClipboardError::Unavailable(_))
        ));
        let mut missing = FixtureClipboard::new("/nonexistent/proqi/clipboard.json".into());
        assert!(matches!(
            missing.read(),
            Err(ClipboardError::Unavailable(_))
        ));
    }
}
