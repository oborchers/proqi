use std::{fs, path::Path};

use crate::ports::invocation::InvocationHarness;

pub(super) const MAX_METADATA_BYTES: u64 = 16 * 1024;
const MAX_NAME_CHARS: usize = 80;
const MAX_DESCRIPTION_CHARS: usize = 180;

#[derive(Default)]
pub(super) struct Metadata {
    pub(super) name: Option<String>,
    pub(super) description: Option<String>,
    pub(super) mode: Option<String>,
    pub(super) hidden: bool,
}

pub(super) fn markdown(path: &Path) -> Option<Metadata> {
    let content = bounded_text(path)?;
    let frontmatter = content.strip_prefix("---\n")?;
    let end = frontmatter.find("\n---")?;
    let mut metadata = Metadata::default();
    for line in frontmatter[..end].lines().take(64) {
        let Some((key, value)) = line.split_once(':') else {
            continue;
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
    Some(metadata)
}

pub(super) fn toml_agent(path: &Path) -> Option<Metadata> {
    let content = bounded_text(path)?;
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

fn bounded_text(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_METADATA_BYTES {
        return None;
    }
    fs::read_to_string(path).ok()
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
