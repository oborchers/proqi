//! Native system clipboard with an OSC 52 write fallback.

use base64::{Engine, engine::general_purpose::STANDARD};

use crate::ports::clipboard::{Clipboard, ClipboardError, ClipboardWrite};

const OSC52_MAX_BYTES: usize = 100_000;

/// Native clipboard adapter with a bounded terminal fallback.
pub struct PlatformClipboard {
    native: Box<dyn NativeClipboard + Send>,
    osc52: bool,
}

impl PlatformClipboard {
    /// Enable native access and the terminal write fallback.
    #[must_use]
    pub fn new() -> Self {
        Self {
            native: Box::new(ArboardNative),
            osc52: true,
        }
    }

    /// Disable OSC 52 for terminals whose policy forbids it.
    #[must_use]
    pub fn without_osc52(mut self) -> Self {
        self.osc52 = false;
        self
    }
}

impl Default for PlatformClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Clipboard for PlatformClipboard {
    fn write(&mut self, content: &str) -> Result<ClipboardWrite, ClipboardError> {
        if self.native.write(content).is_ok() {
            return Ok(ClipboardWrite::Native);
        }
        if !self.osc52 {
            return Err(ClipboardError::Unavailable(
                "native providers failed and OSC 52 is disabled".to_owned(),
            ));
        }
        osc52(content).map(ClipboardWrite::Osc52)
    }

    fn read(&mut self) -> Result<String, ClipboardError> {
        self.native.read().map_err(ClipboardError::Unavailable)
    }
}

trait NativeClipboard {
    fn write(&mut self, content: &str) -> Result<(), String>;
    fn read(&mut self) -> Result<String, String>;
}

struct ArboardNative;

impl NativeClipboard for ArboardNative {
    fn write(&mut self, content: &str) -> Result<(), String> {
        let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
        clipboard
            .set_text(content.to_owned())
            .map_err(|error| error.to_string())
    }

    fn read(&mut self) -> Result<String, String> {
        let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
        clipboard.get_text().map_err(|error| error.to_string())
    }
}

fn osc52(content: &str) -> Result<Vec<u8>, ClipboardError> {
    if content.len() > OSC52_MAX_BYTES {
        return Err(ClipboardError::TooLarge);
    }
    let encoded = STANDARD.encode(content.as_bytes());
    let mut sequence = Vec::with_capacity(encoded.len().saturating_add(8));
    sequence.extend_from_slice(b"\x1b]52;c;");
    sequence.extend_from_slice(encoded.as_bytes());
    sequence.push(0x07);
    Ok(sequence)
}

#[cfg(test)]
mod tests {
    use crate::ports::clipboard::{Clipboard, ClipboardWrite};

    use super::{NativeClipboard, PlatformClipboard};

    #[derive(Default)]
    struct FakeNative {
        content: Option<String>,
        unavailable: bool,
    }

    impl NativeClipboard for FakeNative {
        fn write(&mut self, content: &str) -> Result<(), String> {
            if self.unavailable {
                Err("unavailable".to_owned())
            } else {
                self.content = Some(content.to_owned());
                Ok(())
            }
        }

        fn read(&mut self) -> Result<String, String> {
            self.content.clone().ok_or_else(|| "unavailable".to_owned())
        }
    }

    fn clipboard(native: FakeNative) -> PlatformClipboard {
        PlatformClipboard {
            native: Box::new(native),
            osc52: true,
        }
    }

    #[test]
    fn native_failure_returns_an_exact_bounded_osc52_sequence() {
        let mut clipboard = clipboard(FakeNative {
            unavailable: true,
            ..FakeNative::default()
        });
        assert_eq!(
            clipboard.write("Grüße\n"),
            Ok(ClipboardWrite::Osc52(
                b"\x1b]52;c;R3LDvMOfZQo=\x07".to_vec()
            ))
        );
    }

    #[test]
    fn successful_native_read_preserves_exact_text() {
        let mut clipboard = clipboard(FakeNative {
            content: Some(" exact\r\n".to_owned()),
            unavailable: false,
        });
        assert_eq!(clipboard.read().expect("clipboard"), " exact\r\n");
    }
}
