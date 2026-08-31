//! Conservative normalization of unambiguous terminal file-drop payloads.

use std::path::{Path, PathBuf};

use crate::ui::PastePayload;

pub(super) fn normalize_existing_files(content: &str) -> Option<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(path) = existing_file(trimmed) {
        return Some(display_paths(&[path]));
    }
    let lines = trimmed
        .lines()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.len() > 1 {
        let paths = lines
            .iter()
            .map(|line| existing_file(line.trim()))
            .collect::<Option<Vec<_>>>();
        if let Some(paths) = paths {
            return Some(display_paths(&paths));
        }
    }
    let tokens = split_drop_tokens(trimmed)?;
    if tokens.len() < 2 && tokens.first().is_none_or(|token| token == trimmed) {
        return None;
    }
    let paths = tokens
        .iter()
        .map(|token| existing_file(token))
        .collect::<Option<Vec<_>>>()?;
    Some(display_paths(&paths))
}

pub(super) fn annotate_existing_files(content: &str) -> Option<PastePayload> {
    let content = normalize_existing_files(content)?;
    let mut start = 0;
    let ranges = content
        .split('\n')
        .map(|path| {
            let end = start + path.len();
            let range = start..end;
            let image = is_image_path(Path::new(path));
            let display_name = Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(path)
                .to_owned();
            start = end.saturating_add(1);
            (range, image, display_name)
        })
        .collect();
    Some(PastePayload::attachments(content, ranges))
}

pub(super) fn attachment_payload(path: String, image: bool) -> PastePayload {
    let display_name = Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&path)
        .to_owned();
    let end = path.len();
    PastePayload::attachments(path, vec![(0..end, image, display_name)])
}

fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "avif" | "bmp" | "gif" | "heic" | "jpeg" | "jpg" | "png" | "tiff" | "webp"
            )
        })
}

fn existing_file(value: &str) -> Option<PathBuf> {
    let value = unquote(value);
    let path = if let Some(encoded) = value.strip_prefix("file://") {
        file_url(encoded)?
    } else {
        PathBuf::from(value)
    };
    (path.is_absolute() && path.is_file()).then_some(path)
}

fn unquote(value: &str) -> &str {
    [('\'', '\''), ('"', '"')]
        .into_iter()
        .find_map(|(start, end)| value.strip_prefix(start)?.strip_suffix(end))
        .unwrap_or(value)
}

fn file_url(encoded: &str) -> Option<PathBuf> {
    let local = encoded.strip_prefix("localhost").unwrap_or(encoded);
    if !local.starts_with('/') {
        return None;
    }
    let decoded = percent_decode(local)?;
    Some(PathBuf::from(decoded))
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex(*bytes.get(index + 1)?)?;
            let low = hex(*bytes.get(index + 2)?)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

const fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn split_drop_tokens(value: &str) -> Option<Vec<String>> {
    let tokens = shell_words::split(value).ok()?;
    (!tokens.is_empty()).then_some(tokens)
}

fn display_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| Path::to_string_lossy(path))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use crate::domain::ContentAnnotationKind;

    use super::{annotate_existing_files, normalize_existing_files};

    #[test]
    fn file_urls_quotes_escapes_and_unicode_are_normalized_only_when_real() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let first = temporary.path().join("Grüße 第一 (18).png");
        let second = temporary.path().join("two.txt");
        std::fs::write(&first, b"image").expect("first fixture");
        std::fs::write(&second, b"text").expect("second fixture");

        let encoded = format!(
            "file://{}",
            first
                .to_string_lossy()
                .replace(' ', "%20")
                .replace('ü', "%C3%BC")
        );
        assert_eq!(
            normalize_existing_files(&encoded).as_deref(),
            Some(first.to_string_lossy().as_ref())
        );
        let localhost = format!("file://localhost{}", second.display());
        assert_eq!(
            normalize_existing_files(&localhost).as_deref(),
            Some(second.to_string_lossy().as_ref())
        );
        let escaped_first = first
            .to_string_lossy()
            .replace(' ', "\\ ")
            .replace('(', "\\(")
            .replace(')', "\\)");
        let escaped = format!("{} {}", escaped_first, second.display());
        assert_eq!(
            normalize_existing_files(&escaped),
            Some(format!("{}\n{}", first.display(), second.display()))
        );
        assert_eq!(
            normalize_existing_files(&format!("{}\n{}", first.display(), second.display())),
            Some(format!("{}\n{}", first.display(), second.display()))
        );
        let payload =
            annotate_existing_files(&format!("{}\n{}", first.display(), second.display()))
                .expect("annotated files");
        assert!(matches!(
            payload.annotations[0].kind,
            ContentAnnotationKind::Attachment { image: true, .. }
        ));
        assert!(matches!(
            payload.annotations[1].kind,
            ContentAnnotationKind::Attachment { image: false, .. }
        ));
    }

    #[test]
    fn ghostty_shell_escaped_punctuation_becomes_an_image_attachment() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let file = temporary.path().join("Bild (18) [final] & notes #1.png");
        std::fs::write(&file, b"image").expect("file fixture");
        let escaped = file
            .to_string_lossy()
            .replace(' ', "\\ ")
            .replace('(', "\\(")
            .replace(')', "\\)")
            .replace('[', "\\[")
            .replace(']', "\\]")
            .replace('&', "\\&")
            .replace('#', "\\#");

        let payload = annotate_existing_files(&escaped).expect("annotated image");
        assert_eq!(payload.content, file.to_string_lossy());
        assert_eq!(payload.annotations.len(), 1);
        assert_eq!(payload.annotations[0].start, 0);
        assert_eq!(payload.annotations[0].end, payload.content.len());
        assert!(matches!(
            &payload.annotations[0].kind,
            ContentAnnotationKind::Attachment {
                image: true,
                display_name,
            } if display_name == "Bild (18) [final] & notes #1.png"
        ));
    }

    #[test]
    fn ordinary_or_nonexistent_prompt_text_is_never_rewritten() {
        assert_eq!(normalize_existing_files("write src/main.rs next"), None);
        assert_eq!(
            normalize_existing_files("/definitely/not/a/real/file"),
            None
        );
        assert_eq!(
            normalize_existing_files("file://remote-host/path.png"),
            None
        );
        assert_eq!(normalize_existing_files("s3://bucket/context.png"), None);
        assert_eq!(normalize_existing_files("ftp://host/context.txt"), None);
        assert_eq!(normalize_existing_files("https://host/context.txt"), None);
        assert_eq!(
            normalize_existing_files("/definitely/not/a/real/Bild\\ \\(18\\).png"),
            None
        );
        assert_eq!(normalize_existing_files("write \\(tests\\) next"), None);
        assert_eq!(normalize_existing_files("'unterminated"), None);
        assert_eq!(normalize_existing_files("/tmp/trailing\\"), None);
    }
}
