//! Conservative normalization of unambiguous terminal file-drop payloads.

use std::path::{Path, PathBuf};

use crate::{
    domain::{ContentAnnotation, ContentAnnotationKind},
    ui::PastePayload,
};

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
    let annotations = content
        .split('\n')
        .map(|path| {
            let end = start + path.len();
            let annotation = ContentAnnotation {
                start,
                end,
                kind: ContentAnnotationKind::Attachment {
                    image: is_image_path(Path::new(path)),
                    display_name: Path::new(path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(path)
                        .to_owned(),
                },
            };
            start = end.saturating_add(1);
            annotation
        })
        .collect();
    Some(PastePayload::annotated(content, annotations))
}

pub(super) fn attachment_payload(path: String, image: bool) -> PastePayload {
    let display_name = Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&path)
        .to_owned();
    let end = path.len();
    PastePayload::annotated(
        path,
        vec![ContentAnnotation {
            start: 0,
            end,
            kind: ContentAnnotationKind::Attachment {
                image,
                display_name,
            },
        }],
    )
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
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quote = None;
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        match (quote, character) {
            (Some(active), current) if current == active => quote = None,
            (None, '\'' | '"') => quote = Some(character),
            (None, current) if current.is_whitespace() => push_token(&mut tokens, &mut token),
            (None | Some('"'), '\\') => {
                let next = characters.peek().copied()?;
                if next.is_whitespace() || matches!(next, '\\' | '\'' | '"') {
                    token.push(characters.next()?);
                } else {
                    token.push(character);
                }
            }
            _ => token.push(character),
        }
    }
    if quote.is_some() {
        return None;
    }
    push_token(&mut tokens, &mut token);
    (!tokens.is_empty()).then_some(tokens)
}

fn push_token(tokens: &mut Vec<String>, token: &mut String) {
    if !token.is_empty() {
        tokens.push(std::mem::take(token));
    }
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
        let first = temporary.path().join("Grüße 第一.png");
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
        let escaped = format!(
            "{} {}",
            first.to_string_lossy().replace(' ', "\\ "),
            second.display()
        );
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
        assert_eq!(normalize_existing_files("'unterminated"), None);
    }
}
