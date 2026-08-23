//! Durable materialization of binary prompt attachments.

use std::path::PathBuf;

use thiserror::Error;

use crate::domain::RequestId;

const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;

/// Validated eight-bit RGBA pixels in row-major order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RasterImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl RasterImage {
    /// Validate dimensions and exact RGBA byte length.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, overflowing, or incomplete images.
    pub fn new(width: usize, height: usize, rgba: Vec<u8>) -> Result<Self, AttachmentError> {
        let width = u32::try_from(width).map_err(|_| AttachmentError::InvalidImage)?;
        let height = u32::try_from(height).map_err(|_| AttachmentError::InvalidImage)?;
        let expected = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or(AttachmentError::InvalidImage)?;
        if width == 0 || height == 0 || expected > MAX_IMAGE_BYTES || rgba.len() != expected {
            return Err(AttachmentError::InvalidImage);
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    /// Pixel width.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Pixel height.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Exact row-major RGBA bytes.
    #[must_use]
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

/// Non-destructive attachment materialization failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AttachmentError {
    /// Pixel dimensions or data are invalid or exceed the private-alpha bound.
    #[error("clipboard image is invalid or exceeds 64 MiB")]
    InvalidImage,
    /// The configured attachment root is relative, symlinked, or not a directory.
    #[error("invalid attachment directory: {0}")]
    InvalidDirectory(String),
    /// PNG encoding failed.
    #[error("attachment PNG encoding failed: {0}")]
    Encoding(String),
    /// Atomic filesystem work failed.
    #[error("attachment I/O failed: {0}")]
    Io(String),
}

/// Writes clipboard images into a durable private location.
pub trait AttachmentStore {
    /// Atomically encode one validated clipboard image and return its absolute path.
    ///
    /// # Errors
    ///
    /// Returns a typed error without inserting a path into the board.
    fn save_clipboard_image(
        &mut self,
        request_id: RequestId,
        image: &RasterImage,
    ) -> Result<PathBuf, AttachmentError>;
}

#[cfg(test)]
mod tests {
    use super::{AttachmentError, RasterImage};

    #[test]
    fn raster_image_rejects_empty_incomplete_and_oversized_pixels() {
        assert_eq!(
            RasterImage::new(0, 1, Vec::new()),
            Err(AttachmentError::InvalidImage)
        );
        assert_eq!(
            RasterImage::new(2, 1, vec![0; 4]),
            Err(AttachmentError::InvalidImage)
        );
        assert_eq!(
            RasterImage::new(4_097, 4_097, Vec::new()),
            Err(AttachmentError::InvalidImage)
        );
    }
}
