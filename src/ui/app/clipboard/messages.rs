//! Content-redacted user guidance for clipboard completion failures.

use crate::{application::FailureCode, ui::BoardApp};

impl BoardApp {
    /// Present an application notification returned by a background effect.
    pub fn notify(&mut self, code: FailureCode) {
        let message = match code {
            FailureCode::ClipboardFailed => {
                "clipboard unavailable; use bracketed terminal paste or retry".to_owned()
            }
            FailureCode::ClipboardMetadataUnsupported => {
                "annotated copy is unavailable on this platform; copy only unannotated text or use macOS"
                    .to_owned()
            }
            FailureCode::ContentConflict => {
                "cut cancelled because the selected thought changed; clipboard contains the earlier content"
                    .to_owned()
            }
            FailureCode::StorageFailed => {
                "save failed; press r to retry or w to export recovery".to_owned()
            }
            FailureCode::RecoveryCapacity => "save failed; press w to export recovery".to_owned(),
            _ => code.as_str().to_owned(),
        };
        self.set_error(message);
    }
}
