//! Bounded image-magic and geometry inspection.

use std::{fs::File, io::Read as _};

use crate::ports::screenshot::{ScreenshotBounds, ScreenshotImageType};

const MAX_HEADER_BYTES: usize = 128 * 1024;

pub(super) fn inspect(
    file: &File,
    bounds: ScreenshotBounds,
) -> Option<(ScreenshotImageType, u32, u32)> {
    let clone = file.try_clone().ok()?;
    let mut header = Vec::new();
    clone
        .take(u64::try_from(MAX_HEADER_BYTES).ok()?)
        .read_to_end(&mut header)
        .ok()?;
    let (kind, width, height) = png(&header)
        .or_else(|| jpeg(&header))
        .or_else(|| tiff(&header))?;
    valid_dimensions(width, height, bounds).then_some((kind, width, height))
}

fn valid_dimensions(width: u32, height: u32, bounds: ScreenshotBounds) -> bool {
    width > 0
        && height > 0
        && width <= bounds.max_dimension
        && height <= bounds.max_dimension
        && u64::from(width).saturating_mul(u64::from(height)) <= bounds.max_pixels
}

fn png(bytes: &[u8]) -> Option<(ScreenshotImageType, u32, u32)> {
    let signature = bytes.get(..8)?;
    if signature != b"\x89PNG\r\n\x1a\n" || bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    Some((
        ScreenshotImageType::Png,
        u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?),
        u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?),
    ))
}

fn jpeg(bytes: &[u8]) -> Option<(ScreenshotImageType, u32, u32)> {
    if bytes.get(..2)? != b"\xff\xd8" {
        return None;
    }
    let mut index = 2;
    while index + 4 <= bytes.len() {
        while bytes.get(index) == Some(&0xff) {
            index += 1;
        }
        let marker = *bytes.get(index)?;
        index += 1;
        if matches!(marker, 0xd8 | 0xd9) {
            continue;
        }
        let length = usize::from(u16::from_be_bytes(
            bytes.get(index..index + 2)?.try_into().ok()?,
        ));
        if length < 2 || index.saturating_add(length) > bytes.len() {
            return None;
        }
        if is_jpeg_start_of_frame(marker) && length >= 7 {
            let height = u32::from(u16::from_be_bytes(
                bytes.get(index + 3..index + 5)?.try_into().ok()?,
            ));
            let width = u32::from(u16::from_be_bytes(
                bytes.get(index + 5..index + 7)?.try_into().ok()?,
            ));
            return Some((ScreenshotImageType::Jpeg, width, height));
        }
        index += length;
    }
    None
}

const fn is_jpeg_start_of_frame(marker: u8) -> bool {
    matches!(
        marker,
        0xc0 | 0xc1 | 0xc2 | 0xc3 | 0xc5 | 0xc6 | 0xc7 | 0xc9 | 0xca | 0xcb | 0xcd | 0xce | 0xcf
    )
}

fn tiff(bytes: &[u8]) -> Option<(ScreenshotImageType, u32, u32)> {
    let little = match bytes.get(..4)? {
        b"II\x2a\x00" => true,
        b"MM\x00\x2a" => false,
        _ => return None,
    };
    let ifd = usize::try_from(read_u32(bytes.get(4..8)?, little)).ok()?;
    let count = usize::from(read_u16(bytes.get(ifd..ifd + 2)?, little));
    let mut width = None;
    let mut height = None;
    for entry in 0..count.min(4_096) {
        let start = ifd
            .saturating_add(2)
            .saturating_add(entry.saturating_mul(12));
        let item = bytes.get(start..start + 12)?;
        let tag = read_u16(&item[0..2], little);
        if matches!(tag, 256 | 257) {
            let value = tiff_scalar(item, little)?;
            if tag == 256 {
                width = Some(value);
            } else {
                height = Some(value);
            }
        }
        if width.is_some() && height.is_some() {
            break;
        }
    }
    Some((ScreenshotImageType::Tiff, width?, height?))
}

fn tiff_scalar(entry: &[u8], little: bool) -> Option<u32> {
    let kind = read_u16(entry.get(2..4)?, little);
    let count = read_u32(entry.get(4..8)?, little);
    if count != 1 {
        return None;
    }
    match kind {
        3 => Some(u32::from(read_u16(entry.get(8..10)?, little))),
        4 => Some(read_u32(entry.get(8..12)?, little)),
        _ => None,
    }
}

fn read_u16(bytes: &[u8], little: bool) -> u16 {
    let array = [bytes[0], bytes[1]];
    if little {
        u16::from_le_bytes(array)
    } else {
        u16::from_be_bytes(array)
    }
}

fn read_u32(bytes: &[u8], little: bool) -> u32 {
    let array = [bytes[0], bytes[1], bytes[2], bytes[3]];
    if little {
        u32::from_le_bytes(array)
    } else {
        u32::from_be_bytes(array)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Seek as _, SeekFrom, Write as _};

    use super::inspect;
    use crate::ports::screenshot::{ScreenshotBounds, ScreenshotImageType};

    #[test]
    fn supported_magic_and_geometry_are_checked_without_extensions() {
        let cases = [
            (png(20, 10), ScreenshotImageType::Png),
            (jpeg(30, 20), ScreenshotImageType::Jpeg),
            (tiff(40, 30), ScreenshotImageType::Tiff),
        ];
        for (bytes, expected) in cases {
            let file = file(&bytes);
            assert_eq!(
                inspect(&file, ScreenshotBounds::default()).map(|value| value.0),
                Some(expected)
            );
        }
    }

    #[test]
    fn zero_oversized_and_excessive_pixel_dimensions_are_rejected() {
        let bounds = ScreenshotBounds {
            max_dimension: 100,
            max_pixels: 2_000,
            ..ScreenshotBounds::default()
        };
        assert!(inspect(&file(&png(0, 10)), bounds).is_none());
        assert!(inspect(&file(&png(101, 10)), bounds).is_none());
        assert!(inspect(&file(&png(50, 50)), bounds).is_none());
    }

    fn file(bytes: &[u8]) -> std::fs::File {
        let mut file = tempfile::tempfile().expect("temporary image");
        file.write_all(bytes).expect("write image");
        file.seek(SeekFrom::Start(0)).expect("rewind image");
        file
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0; 64];
        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        bytes[12..16].copy_from_slice(b"IHDR");
        bytes[16..20].copy_from_slice(&width.to_be_bytes());
        bytes[20..24].copy_from_slice(&height.to_be_bytes());
        bytes
    }

    fn jpeg(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = vec![0xff, 0xd8, 0xff, 0xc0, 0x00, 0x08, 0x08];
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&[3, 0]);
        bytes
    }

    fn tiff(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::from(b"II\x2a\x00\x08\x00\x00\x00\x02\x00".as_slice());
        for (tag, value) in [(256_u16, width), (257_u16, height)] {
            bytes.extend_from_slice(&tag.to_le_bytes());
            bytes.extend_from_slice(&4_u16.to_le_bytes());
            bytes.extend_from_slice(&1_u32.to_le_bytes());
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }
}
