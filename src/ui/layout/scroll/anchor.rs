//! Projection-stable coordinates for Board content rows.

use super::ThoughtRows;
use crate::ui::projection::PresentedThought;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum ContentAnchor {
    Canonical(usize),
    Projection {
        annotation_index: usize,
        row: usize,
        canonical_byte: usize,
    },
}

impl ContentAnchor {
    pub(super) const fn canonical_byte(self) -> usize {
        match self {
            Self::Canonical(byte)
            | Self::Projection {
                canonical_byte: byte,
                ..
            } => byte,
        }
    }
}

pub(super) fn content_row_anchors(
    thought: &PresentedThought,
    row_starts: &[usize],
) -> Vec<ContentAnchor> {
    let mut projection_rows = Vec::<(usize, usize)>::new();
    row_starts
        .iter()
        .map(|start| {
            let canonical_byte =
                crate::ui::projection::unproject_byte(*start, &thought.presentation.substitutions);
            let substitution = thought
                .presentation
                .substitutions
                .iter()
                .find(|substitution| {
                    let display_only_start = if substitution.collapsed {
                        substitution.start
                    } else {
                        substitution.content_end
                    };
                    *start >= display_only_start && *start < substitution.end
                });
            let Some(substitution) = substitution else {
                return ContentAnchor::Canonical(canonical_byte);
            };
            let row = projection_row(&mut projection_rows, substitution.annotation_index);
            ContentAnchor::Projection {
                annotation_index: substitution.annotation_index,
                row,
                canonical_byte,
            }
        })
        .collect()
}

fn projection_row(rows: &mut Vec<(usize, usize)>, annotation_index: usize) -> usize {
    let Some(position) = rows
        .iter()
        .position(|(index, _)| *index == annotation_index)
    else {
        rows.push((annotation_index, 1));
        return 0;
    };
    let row = rows[position].1;
    rows[position].1 = row.saturating_add(1);
    row
}

pub(super) fn content_row_for_anchor(thought: &ThoughtRows, anchor: ContentAnchor) -> usize {
    let visible = thought.content_rows;
    if let ContentAnchor::Projection {
        annotation_index,
        row,
        ..
    } = anchor
    {
        let matching = thought
            .row_anchors
            .iter()
            .take(visible)
            .enumerate()
            .filter_map(|(index, candidate)| {
                matches!(
                    candidate,
                    ContentAnchor::Projection {
                        annotation_index: candidate_index,
                        ..
                    } if *candidate_index == annotation_index
                )
                .then_some(index)
            })
            .collect::<Vec<_>>();
        if let Some(index) = matching.get(row).or_else(|| matching.last()) {
            return *index;
        }
    }
    canonical_row(thought, visible, anchor.canonical_byte())
}

fn canonical_row(thought: &ThoughtRows, visible: usize, canonical_byte: usize) -> usize {
    thought
        .row_anchors
        .iter()
        .take(visible)
        .enumerate()
        .filter(|(_, candidate)| {
            matches!(candidate, ContentAnchor::Canonical(_))
                && candidate.canonical_byte() <= canonical_byte
        })
        .map(|(index, _)| index)
        .next_back()
        .or_else(|| {
            thought
                .row_anchors
                .iter()
                .take(visible)
                .rposition(|candidate| candidate.canonical_byte() <= canonical_byte)
        })
        .unwrap_or(0)
}
