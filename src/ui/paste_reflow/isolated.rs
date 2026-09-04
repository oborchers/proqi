//! Boundary-safe orchestration for transformable large-paste envelopes.

use std::ops::Range;

use crate::ports::editor::{TextChange, TextChangeSet};

use super::{ReflowError, ReflowedText};

pub(super) fn reflow(
    content: &str,
    protected: &[Range<usize>],
    isolated: &[Range<usize>],
) -> Result<ReflowedText, ReflowError> {
    if super::contains_unsupported_control(content) {
        return Ok(super::unchanged(content));
    }
    let newline = super::preferred_newline(content);
    if isolated.is_empty() {
        return super::reflow_slice(content, protected, newline);
    }
    let points = partition_points(content, isolated)?;
    let mut state = ReflowState::new(content.len(), isolated);
    let mut isolated_index = 0;
    for pair in points.windows(2) {
        let partition = pair[0]..pair[1];
        let owner = isolated
            .get(isolated_index)
            .filter(|range| **range == partition)
            .map(|_| isolated_index);
        if owner.is_some() {
            isolated_index += 1;
        }
        state.append_partition(content, partition, protected, newline, owner)?;
    }
    state.finish(content)
}

struct OwnedRange {
    range: Range<usize>,
    owner: Option<usize>,
}

struct ReflowState {
    output: String,
    mapped: Vec<(Range<usize>, Range<usize>)>,
    pending_whitespace: Vec<OwnedRange>,
    protected_index: usize,
    has_content: bool,
    isolated_old: Vec<Range<usize>>,
    isolated_new: Vec<Option<Range<usize>>>,
}

impl ReflowState {
    fn new(capacity: usize, isolated: &[Range<usize>]) -> Self {
        Self {
            output: String::with_capacity(capacity),
            mapped: Vec::new(),
            pending_whitespace: Vec::new(),
            protected_index: 0,
            has_content: false,
            isolated_old: isolated.to_vec(),
            isolated_new: vec![None; isolated.len()],
        }
    }

    fn append_partition(
        &mut self,
        content: &str,
        partition: Range<usize>,
        protected: &[Range<usize>],
        newline: &str,
        owner: Option<usize>,
    ) -> Result<(), ReflowError> {
        let core = partition_core(&content[partition.clone()]);
        let old_base = partition.start + core.start;
        let old_end = partition.start + core.end;
        self.queue_whitespace(partition.start..old_base, owner);
        if old_base < old_end {
            self.flush_whitespace(content, newline, false);
            let local =
                local_protected_ranges(protected, &mut self.protected_index, old_base..old_end);
            let transformed = super::reflow_slice(&content[old_base..old_end], &local, newline)?;
            let new_base = self.output.len();
            self.output.push_str(&transformed.content);
            self.note_owner(owner, new_base..self.output.len());
            self.mapped
                .extend(transformed.changes.as_slice().iter().map(|change| {
                    (
                        offset_range(change.old_range(), old_base),
                        offset_range(change.new_range(), new_base),
                    )
                }));
            self.has_content = true;
        }
        self.queue_whitespace(old_end..partition.end, owner);
        Ok(())
    }

    fn queue_whitespace(&mut self, range: Range<usize>, owner: Option<usize>) {
        if !range.is_empty() {
            self.pending_whitespace.push(OwnedRange { range, owner });
        }
    }

    fn note_owner(&mut self, owner: Option<usize>, new: Range<usize>) {
        let Some(owner) = owner else {
            return;
        };
        if let Some(existing) = &mut self.isolated_new[owner] {
            existing.start = existing.start.min(new.start);
            existing.end = existing.end.max(new.end);
        } else {
            self.isolated_new[owner] = Some(new);
        }
    }

    fn flush_whitespace(&mut self, content: &str, newline: &str, trailing: bool) {
        if self.pending_whitespace.is_empty() {
            return;
        }
        let separator = if !self.has_content || trailing {
            String::new()
        } else if count_breaks(content, &self.pending_whitespace) >= 2 {
            newline.repeat(2)
        } else {
            " ".to_owned()
        };
        let new_start = self.output.len();
        self.output.push_str(&separator);
        let pending = std::mem::take(&mut self.pending_whitespace);
        let old = pending[0].range.start..pending[pending.len() - 1].range.end;
        if content[old.clone()] != separator {
            self.mapped.push((old, new_start..self.output.len()));
        }
        for (index, owned) in pending.into_iter().enumerate() {
            let new = if index == 0 {
                new_start..self.output.len()
            } else {
                self.output.len()..self.output.len()
            };
            self.note_owner(owned.owner, new);
        }
    }

    fn finish(mut self, content: &str) -> Result<ReflowedText, ReflowError> {
        self.flush_whitespace(content, super::preferred_newline(content), true);
        self.mapped.sort_by_key(|(old, _)| old.start);
        let mapped = coalesce_mappings(self.mapped);
        let changes = mapped
            .into_iter()
            .map(|(old, new)| TextChange::new(content, &self.output, old, new))
            .collect::<Result<Vec<_>, _>>()?;
        let changes = TextChangeSet::new(content, &self.output, changes)?;
        let isolated = self
            .isolated_old
            .into_iter()
            .zip(self.isolated_new)
            .filter_map(|(old, new)| new.map(|new| (old, new)))
            .collect();
        Ok(ReflowedText {
            content: self.output,
            changes,
            isolated,
        })
    }
}

fn count_breaks(content: &str, pending: &[OwnedRange]) -> usize {
    pending
        .iter()
        .map(|owned| {
            content[owned.range.clone()]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
        })
        .sum()
}

fn coalesce_mappings(
    mappings: Vec<(Range<usize>, Range<usize>)>,
) -> Vec<(Range<usize>, Range<usize>)> {
    let mut output: Vec<(Range<usize>, Range<usize>)> = Vec::new();
    for (old, new) in mappings {
        if let Some((prior_old, prior_new)) = output.last_mut()
            && (old.start == prior_old.end || new.start == prior_new.end)
        {
            prior_old.end = old.end;
            prior_new.end = new.end;
        } else {
            output.push((old, new));
        }
    }
    output
}

fn partition_points(content: &str, isolated: &[Range<usize>]) -> Result<Vec<usize>, ReflowError> {
    let mut points = Vec::with_capacity(isolated.len().saturating_mul(2).saturating_add(2));
    points.push(0);
    let mut prior_end = 0;
    for range in isolated {
        if range.start < prior_end
            || range.start > range.end
            || range.end > content.len()
            || !content.is_char_boundary(range.start)
            || !content.is_char_boundary(range.end)
        {
            return Err(ReflowError::InvalidBoundary);
        }
        points.extend([range.start, range.end]);
        prior_end = range.end;
    }
    points.push(content.len());
    points.dedup();
    Ok(points)
}

fn partition_core(value: &str) -> Range<usize> {
    let start = value
        .char_indices()
        .find_map(|(index, character)| (!is_boundary_whitespace(character)).then_some(index))
        .unwrap_or(value.len());
    let end = value
        .char_indices()
        .rev()
        .find_map(|(index, character)| {
            (!is_boundary_whitespace(character)).then_some(index + character.len_utf8())
        })
        .unwrap_or(start);
    start..end.max(start)
}

fn is_boundary_whitespace(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\r' | '\n')
}

fn local_protected_ranges(
    protected: &[Range<usize>],
    index: &mut usize,
    core: Range<usize>,
) -> Vec<Range<usize>> {
    while protected
        .get(*index)
        .is_some_and(|range| range.end <= core.start)
    {
        *index += 1;
    }
    let mut local = Vec::new();
    while let Some(range) = protected.get(*index) {
        if range.start >= core.end {
            break;
        }
        let start = range.start.max(core.start) - core.start;
        let end = range.end.min(core.end) - core.start;
        if start < end {
            local.push(start..end);
        }
        if range.end > core.end {
            break;
        }
        *index += 1;
    }
    local
}

fn offset_range(range: Range<usize>, base: usize) -> Range<usize> {
    base + range.start..base + range.end
}
