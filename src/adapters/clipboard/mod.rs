//! Native system clipboard with an OSC 52 write fallback.

use base64::{Engine, engine::general_purpose::STANDARD};

use crate::ports::{
    attachment::RasterImage,
    clipboard::{Clipboard, ClipboardContent, ClipboardError, ClipboardWrite},
};

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
            native: Box::new(ArboardNative::default()),
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

    fn read(&mut self) -> Result<ClipboardContent, ClipboardError> {
        match self.native.read_image() {
            Ok(image) => return Ok(ClipboardContent::Image(image)),
            Err(NativeReadError::InvalidImage) => return Err(ClipboardError::InvalidImage),
            Err(NativeReadError::Unavailable(_)) => {}
        }
        self.native
            .read_text()
            .map(ClipboardContent::Text)
            .map_err(|error| match error {
                NativeReadError::Unavailable(message) => ClipboardError::Unavailable(message),
                NativeReadError::InvalidImage => ClipboardError::InvalidImage,
            })
    }
}

trait NativeClipboard {
    fn write(&mut self, content: &str) -> Result<(), String>;
    fn read_text(&mut self) -> Result<String, NativeReadError>;
    fn read_image(&mut self) -> Result<RasterImage, NativeReadError>;
}

enum NativeReadError {
    Unavailable(String),
    InvalidImage,
}

#[derive(Default)]
struct ArboardNative {
    clipboard: Option<arboard::Clipboard>,
}

impl ArboardNative {
    fn clipboard(&mut self) -> Result<&mut arboard::Clipboard, String> {
        if self.clipboard.is_none() {
            self.clipboard = Some(arboard::Clipboard::new().map_err(|error| error.to_string())?);
        }
        self.clipboard
            .as_mut()
            .ok_or_else(|| "native clipboard initialization failed".to_owned())
    }
}

impl NativeClipboard for ArboardNative {
    fn write(&mut self, content: &str) -> Result<(), String> {
        self.clipboard()?
            .set_text(content.to_owned())
            .map_err(|error| error.to_string())
    }

    fn read_text(&mut self) -> Result<String, NativeReadError> {
        self.clipboard()
            .map_err(NativeReadError::Unavailable)?
            .get_text()
            .map_err(native_unavailable)
    }

    fn read_image(&mut self) -> Result<RasterImage, NativeReadError> {
        let image = self
            .clipboard()
            .map_err(NativeReadError::Unavailable)?
            .get_image()
            .map_err(native_unavailable)?;
        RasterImage::new(image.width, image.height, image.bytes.into_owned())
            .map_err(|_| NativeReadError::InvalidImage)
    }
}

fn native_unavailable(error: impl ToString) -> NativeReadError {
    let message = error.to_string();
    drop(error);
    NativeReadError::Unavailable(message)
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
    use crate::ports::{
        attachment::RasterImage,
        clipboard::{Clipboard, ClipboardContent, ClipboardWrite},
    };

    use super::{NativeClipboard, NativeReadError, PlatformClipboard};

    #[derive(Default)]
    struct FakeNative {
        content: Option<String>,
        image: Option<RasterImage>,
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

        fn read_text(&mut self) -> Result<String, NativeReadError> {
            self.content
                .clone()
                .ok_or_else(|| NativeReadError::Unavailable("unavailable".to_owned()))
        }

        fn read_image(&mut self) -> Result<RasterImage, NativeReadError> {
            self.image
                .clone()
                .ok_or_else(|| NativeReadError::Unavailable("unavailable".to_owned()))
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
            image: None,
            unavailable: false,
        });
        assert_eq!(
            clipboard.read().expect("clipboard"),
            ClipboardContent::Text(" exact\r\n".to_owned())
        );
    }

    #[test]
    fn native_image_is_preferred_and_remains_exact_rgba() {
        let image = RasterImage::new(1, 1, vec![1, 2, 3, 255]).expect("image");
        let mut clipboard = clipboard(FakeNative {
            content: Some("fallback text".to_owned()),
            image: Some(image.clone()),
            unavailable: false,
        });
        assert_eq!(
            clipboard.read().expect("clipboard"),
            ClipboardContent::Image(image)
        );
    }
}
