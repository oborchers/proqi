//! Render-only exact invocation ranges.

use std::{collections::BTreeSet, ops::Range};

use crate::ui::app::BoardApp;

use super::{builtins, compatibility, plausible};

const MAX_HIGHLIGHTS: usize = 128;

impl BoardApp {
    pub(in crate::ui) fn invocation_ranges(&self, content: &str) -> Vec<Range<usize>> {
        let tokens = self.highlight_tokens();
        scan_content(content, &tokens)
    }

    fn highlight_tokens(&self) -> BTreeSet<String> {
        self.invocation_project
            .iter()
            .chain(&self.invocation_global)
            .flat_map(|entry| &entry.forms)
            .filter(|form| compatibility::supports_form(self, form))
            .map(|form| form.token.clone())
            .chain(builtins::tokens(self).map(str::to_owned))
            .collect()
    }
}

fn scan_content(content: &str, tokens: &BTreeSet<String>) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut line_start = 0;
    let mut in_fence = false;
    for line in content.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        } else if !in_fence {
            scan_line(line, line_start, tokens, &mut ranges);
            if ranges.len() >= MAX_HIGHLIGHTS {
                break;
            }
        }
        line_start = line_start.saturating_add(line.len());
    }
    ranges
}

fn scan_line(
    line: &str,
    line_start: usize,
    tokens: &BTreeSet<String>,
    ranges: &mut Vec<Range<usize>>,
) {
    for (start, character) in line.char_indices() {
        if ranges.len() >= MAX_HIGHLIGHTS || !matches!(character, '$' | '/' | '@') {
            continue;
        }
        let absolute_start = line_start.saturating_add(start);
        if character == '/' && absolute_start != 0 {
            continue;
        }
        if character != '/' && !has_token_boundary(line, start) {
            continue;
        }
        let body_start = start.saturating_add(character.len_utf8());
        let body_len = line[body_start..]
            .char_indices()
            .take_while(|(_, current)| is_token_character(*current))
            .map(|(offset, current)| offset.saturating_add(current.len_utf8()))
            .last()
            .unwrap_or(0);
        let end = body_start.saturating_add(body_len);
        let Some(token) = line.get(start..end) else {
            continue;
        };
        if plausible(token) && tokens.contains(token) {
            ranges.push(absolute_start..line_start.saturating_add(end));
        }
    }
}

fn has_token_boundary(line: &str, start: usize) -> bool {
    line.get(..start)
        .and_then(|prefix| prefix.chars().next_back())
        .is_none_or(|previous| {
            previous.is_whitespace()
                || (!previous.is_alphanumeric()
                    && !matches!(previous, '_' | '.' | '/' | '\\' | ':' | '$' | '@'))
        })
}

fn is_token_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '-' | '_' | ':' | '/' | '.')
}

#[cfg(test)]
#[path = "highlight_tests.rs"]
mod tests;
