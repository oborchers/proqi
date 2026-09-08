//! Precise editor positions within already classified reflow replacements.
//!
//! Annotation mapping retains compact group replacements. Editor projection
//! refines only those replacements, preserving the unchanged scalar spans.
//! This describes existing output; it never chooses or formats content.

use crate::ports::editor::{TextChange, TextChangeSet};

use super::ReflowError;

pub(in crate::ui) fn position_changes(
    before: &str,
    after: &str,
    groups: &TextChangeSet,
) -> Result<TextChangeSet, ReflowError> {
    let mut changes = Vec::new();
    for group in groups.as_slice() {
        let old = group.old_range();
        let new = group.new_range();
        let mut source = old.start;
        let mut target = new.start;
        while source < old.end || target < new.end {
            let left = before[source..old.end].chars().next();
            let right = after[target..new.end].chars().next();
            if let Some(character) = left
                && left == right
            {
                source += character.len_utf8();
                target += character.len_utf8();
                continue;
            }
            let start = (source, target);
            source += layout_prefix(&before[source..old.end]);
            target += layout_prefix(&after[target..new.end]);
            if start == (source, target) {
                return Err(ReflowError::InvalidBoundary);
            }
            changes.push((start.0..source, start.1..target));
        }
    }
    let changes = super::isolated::coalesce_mappings(changes)
        .into_iter()
        .map(|(old, new)| TextChange::new(before, after, old, new))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(TextChangeSet::new(before, after, changes)?)
}

fn layout_prefix(value: &str) -> usize {
    value
        .bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        .count()
}
