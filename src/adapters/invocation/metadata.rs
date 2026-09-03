use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader, Read},
    path::Path,
};

use crate::ports::invocation::InvocationHarness;

pub(super) const MAX_MARKDOWN_FRONTMATTER_BYTES: usize = 64 * 1024;
const MAX_COMPLETE_METADATA_BYTES: u64 = 16 * 1024;
const MAX_METADATA_LINES: usize = 64;
const MAX_NAME_CHARS: usize = 80;
const MAX_DESCRIPTION_CHARS: usize = 180;

#[derive(Default)]
pub(super) struct Metadata {
    pub(super) name: Option<String>,
    pub(super) description: Option<String>,
    pub(super) mode: Option<String>,
    pub(super) hidden: bool,
}

pub(super) enum MarkdownMetadata {
    Absent,
    Parsed(Metadata),
    Invalid,
}

pub(super) fn markdown(path: &Path) -> MarkdownMetadata {
    let Ok(file_metadata) = fs::metadata(path) else {
        return MarkdownMetadata::Invalid;
    };
    if !file_metadata.is_file() {
        return MarkdownMetadata::Invalid;
    }
    let Ok(mut file) = File::open(path) else {
        return MarkdownMetadata::Invalid;
    };
    markdown_reader(&mut file)
}

fn markdown_reader(reader: &mut impl Read) -> MarkdownMetadata {
    let mut reader = BufReader::with_capacity(1, reader);
    let mut consumed = 0;
    match opening_delimiter(&mut reader, &mut consumed) {
        Ok(true) => {}
        Ok(false) => return MarkdownMetadata::Absent,
        Err(()) => return MarkdownMetadata::Invalid,
    }
    let mut metadata = Metadata::default();
    let mut line_count = 0;
    loop {
        let Ok(Some(line)) = bounded_line(&mut reader, &mut consumed) else {
            return MarkdownMetadata::Invalid;
        };
        if is_closing_delimiter(&line) {
            return MarkdownMetadata::Parsed(metadata);
        }
        let Ok(text) = std::str::from_utf8(without_line_ending(&line)) else {
            return MarkdownMetadata::Invalid;
        };
        if line_count < MAX_METADATA_LINES {
            parse_line(&mut metadata, text);
        }
        line_count = line_count.saturating_add(1);
    }
}

pub(super) fn toml_agent(path: &Path) -> Option<Metadata> {
    let content = complete_bounded_text(path)?;
    let value = toml::from_str::<toml::Value>(&content).ok()?;
    Some(Metadata {
        name: value
            .get("name")
            .and_then(toml::Value::as_str)
            .and_then(clean_name),
        description: value
            .get("description")
            .and_then(toml::Value::as_str)
            .and_then(clean_description),
        ..Metadata::default()
    })
}

pub(super) fn filename_name(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|name| name.to_str())
        .and_then(clean_name)
}

pub(super) fn command_name(root: &Path, path: &Path, harness: InvocationHarness) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?.with_extension("");
    let parts = relative
        .components()
        .filter_map(|part| part.as_os_str().to_str())
        .map(clean_name)
        .collect::<Option<Vec<_>>>()?;
    let separator = if harness == InvocationHarness::ClaudeCode {
        ":"
    } else {
        "/"
    };
    clean_name(&parts.join(separator))
}

fn complete_bounded_text(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_COMPLETE_METADATA_BYTES {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn opening_delimiter(reader: &mut impl BufRead, consumed: &mut usize) -> Result<bool, ()> {
    for expected in b"---" {
        match read_byte(reader, consumed)? {
            Some(byte) if byte == *expected => {}
            Some(_) | None => return Ok(false),
        }
    }
    match read_byte(reader, consumed)? {
        Some(b'\n') => Ok(true),
        Some(b'\r') => Ok(read_byte(reader, consumed)? == Some(b'\n')),
        Some(_) | None => Ok(false),
    }
}

fn read_byte(reader: &mut impl BufRead, consumed: &mut usize) -> Result<Option<u8>, ()> {
    if *consumed >= MAX_MARKDOWN_FRONTMATTER_BYTES {
        return Ok(None);
    }
    let available = reader.fill_buf().map_err(|_error| ())?;
    let Some(byte) = available.first().copied() else {
        return Ok(None);
    };
    reader.consume(1);
    *consumed = consumed.saturating_add(1);
    Ok(Some(byte))
}

fn bounded_line(
    reader: &mut impl BufRead,
    consumed: &mut usize,
) -> Result<Option<Vec<u8>>, io::Error> {
    let mut line = Vec::new();
    loop {
        if *consumed >= MAX_MARKDOWN_FRONTMATTER_BYTES {
            return Ok(None);
        }
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok((!line.is_empty()).then_some(line));
        }
        let remaining = MAX_MARKDOWN_FRONTMATTER_BYTES.saturating_sub(*consumed);
        let bounded = &available[..available.len().min(remaining)];
        let length = bounded
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bounded.len(), |index| index.saturating_add(1));
        line.extend_from_slice(&bounded[..length]);
        reader.consume(length);
        *consumed = consumed.saturating_add(length);
        if line.ends_with(b"\n") {
            return Ok(Some(line));
        }
    }
}

fn is_closing_delimiter(line: &[u8]) -> bool {
    without_line_ending(line) == b"---"
}

fn without_line_ending(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

fn parse_line(metadata: &mut Metadata, line: &str) {
    let Some((key, value)) = line.split_once(':') else {
        return;
    };
    let value = scalar(value);
    match key.trim() {
        "name" => metadata.name = clean_name(&value),
        "description" => metadata.description = clean_description(&value),
        "mode" => metadata.mode = clean_name(&value).map(|value| value.to_lowercase()),
        "hidden" | "disable" => metadata.hidden = matches!(value.as_str(), "true" | "yes"),
        _ => {}
    }
}

fn scalar(value: &str) -> String {
    value
        .trim()
        .trim_matches(|character| matches!(character, '\'' | '"'))
        .to_owned()
}

pub(super) fn clean_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()
        && trimmed.chars().count() <= MAX_NAME_CHARS
        && !trimmed
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        && trimmed.chars().all(|character| {
            character.is_alphanumeric() || matches!(character, '-' | '_' | ':' | '/' | '.')
        }))
    .then(|| trimmed.to_owned())
}

fn clean_description(value: &str) -> Option<String> {
    let cleaned = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.is_empty() {
        return None;
    }
    Some(cleaned.chars().take(MAX_DESCRIPTION_CHARS).collect())
}

#[cfg(test)]
mod tests;
