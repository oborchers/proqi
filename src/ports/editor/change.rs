//! Validated text changes shared by editor implementations and application consumers.

use std::ops::Range;

/// Document coordinate space addressed by a text-change range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextCoordinateSpace {
    /// The complete document before the transaction.
    Before,
    /// The complete document after the transaction.
    After,
}

/// Which edge receives an offset that lies inside replaced text or at an insertion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OffsetAffinity {
    /// Associate the offset with the content before the change.
    Before,
    /// Associate the offset with the content after the change.
    After,
}

/// Validation failure for an explicit text transaction.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TextChangeError {
    /// A range is reversed or outside its complete document.
    #[error("invalid {space:?} text range {start}..{end} for document length {len}")]
    InvalidRange {
        /// Coordinate space containing the range.
        space: TextCoordinateSpace,
        /// Inclusive range start.
        start: usize,
        /// Exclusive range end.
        end: usize,
        /// Complete document length.
        len: usize,
    },
    /// A range or mapped offset splits a UTF-8 scalar value.
    #[error("{space:?} text offset {offset} is not a UTF-8 character boundary")]
    InvalidUtf8Boundary {
        /// Coordinate space containing the offset.
        space: TextCoordinateSpace,
        /// Invalid byte offset.
        offset: usize,
    },
    /// Changes are not ordered or overlap in either coordinate space.
    #[error("text change {index} is unordered or overlaps its predecessor")]
    UnorderedOrOverlapping {
        /// Index of the invalid change.
        index: usize,
    },
    /// The ranges do not preserve the untouched text between changes.
    #[error("text change {index} has inconsistent before/after coordinates")]
    InconsistentCoordinates {
        /// Index of the invalid change, or the number of changes for the final suffix.
        index: usize,
    },
    /// One reported entry does not change the addressed text.
    #[error("text change {index} reports identical old and resulting text")]
    UnchangedEntry {
        /// Index of the ineffective change.
        index: usize,
    },
    /// Different documents were paired with an empty change sequence.
    #[error("changed content is not represented by the text change sequence")]
    UnrepresentedContent,
    /// An offset cannot be mapped through this transaction.
    #[error("before text offset {offset} exceeds document length {len}")]
    InvalidOffset {
        /// Requested offset.
        offset: usize,
        /// Complete before-document length.
        len: usize,
    },
    /// The supplied mapping document does not match the transaction length.
    #[error("mapping document length {actual} does not match expected length {expected}")]
    MappingDocumentLength {
        /// Length recorded by the transaction.
        expected: usize,
        /// Length supplied to mapping.
        actual: usize,
    },
}

/// One exact replacement expressed in both complete-document coordinate spaces.
///
/// `old_range` addresses the document before the transaction and `new_range`
/// addresses the resulting document. Empty old ranges are insertions; empty new
/// ranges are deletions. This type deliberately carries no move identity: equal
/// text elsewhere is not evidence that content moved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextChange {
    old_range: Range<usize>,
    new_range: Range<usize>,
}

impl TextChange {
    /// Construct one range pair on valid UTF-8 boundaries.
    ///
    /// # Errors
    ///
    /// Returns [`TextChangeError`] when either range is reversed, outside its
    /// document, or splits a UTF-8 scalar value.
    pub fn new(
        before: &str,
        after: &str,
        old_range: Range<usize>,
        new_range: Range<usize>,
    ) -> Result<Self, TextChangeError> {
        validate_range(before, &old_range, TextCoordinateSpace::Before)?;
        validate_range(after, &new_range, TextCoordinateSpace::After)?;
        Ok(Self {
            old_range,
            new_range,
        })
    }

    /// Range in the complete document before the transaction.
    #[must_use]
    pub fn old_range(&self) -> Range<usize> {
        self.old_range.clone()
    }

    /// Range in the complete resulting document.
    #[must_use]
    pub fn new_range(&self) -> Range<usize> {
        self.new_range.clone()
    }

    /// Whether this change inserts without removing text.
    #[must_use]
    pub fn is_insertion(&self) -> bool {
        self.old_range.is_empty()
    }
}

/// An atomic ordered sequence of disjoint changes between two complete documents.
///
/// Entries are ordered by their ranges in the before document. Both range sets
/// are non-overlapping, and untouched gaps are byte-for-byte identical. This
/// makes old-to-new offset mapping deterministic without diffing full content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextChangeSet {
    changes: Vec<TextChange>,
    before_len: usize,
    after_len: usize,
}

impl TextChangeSet {
    /// Validate a complete explicit transaction.
    ///
    /// # Errors
    ///
    /// Returns [`TextChangeError`] for invalid UTF-8 boundaries, unordered or
    /// overlapping changes, ineffective entries, or range pairs that do not
    /// exactly preserve every unreported gap between `before` and `after`.
    pub fn new(
        before: &str,
        after: &str,
        changes: Vec<TextChange>,
    ) -> Result<Self, TextChangeError> {
        if changes.is_empty() {
            if before != after {
                return Err(TextChangeError::UnrepresentedContent);
            }
            return Ok(Self::unchanged(before.len()));
        }
        validate_changes(before, after, &changes)?;
        Ok(Self {
            changes,
            before_len: before.len(),
            after_len: after.len(),
        })
    }

    /// Construct an unchanged outcome for a document of `len` bytes.
    #[must_use]
    pub const fn unchanged(len: usize) -> Self {
        Self {
            changes: Vec::new(),
            before_len: len,
            after_len: len,
        }
    }

    /// Report an explicit whole-document replacement.
    ///
    /// Equal documents produce an unchanged set. This constructor is infallible
    /// because both complete-string ranges necessarily use valid UTF-8 boundaries.
    #[must_use]
    pub fn replace_all(before: &str, after: &str) -> Self {
        if before == after {
            return Self::unchanged(before.len());
        }
        Self {
            changes: vec![TextChange {
                old_range: 0..before.len(),
                new_range: 0..after.len(),
            }],
            before_len: before.len(),
            after_len: after.len(),
        }
    }

    /// Ordered explicit changes.
    #[must_use]
    pub fn as_slice(&self) -> &[TextChange] {
        &self.changes
    }

    /// Number of explicit changes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.changes.len()
    }

    /// Whether the transaction changes no content.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Change ranges for the inverse transaction, without inspecting document text.
    #[must_use]
    pub fn inverse(&self) -> Self {
        Self {
            changes: self
                .changes
                .iter()
                .map(|change| TextChange {
                    old_range: change.new_range.clone(),
                    new_range: change.old_range.clone(),
                })
                .collect(),
            before_len: self.after_len,
            after_len: self.before_len,
        }
    }

    /// Map one valid before-document byte offset into the resulting document.
    ///
    /// Offsets inside replaced text map to the chosen edge. At a pure insertion,
    /// [`OffsetAffinity::Before`] stays before inserted text and
    /// [`OffsetAffinity::After`] moves after it.
    ///
    /// # Errors
    ///
    /// Returns [`TextChangeError`] when `before` has the wrong length or `offset`
    /// is outside it or splits a UTF-8 scalar value.
    pub fn map_old_offset(
        &self,
        before: &str,
        offset: usize,
        affinity: OffsetAffinity,
    ) -> Result<usize, TextChangeError> {
        if before.len() != self.before_len {
            return Err(TextChangeError::MappingDocumentLength {
                expected: self.before_len,
                actual: before.len(),
            });
        }
        if offset > self.before_len {
            return Err(TextChangeError::InvalidOffset {
                offset,
                len: self.before_len,
            });
        }
        if !before.is_char_boundary(offset) {
            return Err(TextChangeError::InvalidUtf8Boundary {
                space: TextCoordinateSpace::Before,
                offset,
            });
        }
        Ok(self.map_valid_offset(offset, affinity))
    }

    fn map_valid_offset(&self, offset: usize, affinity: OffsetAffinity) -> usize {
        let mut old_cursor = 0;
        let mut new_cursor = 0;
        for change in &self.changes {
            if offset < change.old_range.start {
                return new_cursor + offset.saturating_sub(old_cursor);
            }
            if change.old_range.is_empty() && offset == change.old_range.start {
                return match affinity {
                    OffsetAffinity::Before => change.new_range.start,
                    OffsetAffinity::After => change.new_range.end,
                };
            }
            if offset < change.old_range.end {
                return match affinity {
                    OffsetAffinity::Before => change.new_range.start,
                    OffsetAffinity::After => change.new_range.end,
                };
            }
            old_cursor = change.old_range.end;
            new_cursor = change.new_range.end;
        }
        new_cursor + offset.saturating_sub(old_cursor)
    }
}

fn validate_changes(
    before: &str,
    after: &str,
    changes: &[TextChange],
) -> Result<(), TextChangeError> {
    let mut previous_old_end = 0;
    let mut previous_new_end = 0;
    let mut previous_old_start = None;
    let mut previous_new_start = None;
    for (index, change) in changes.iter().enumerate() {
        validate_range(before, &change.old_range, TextCoordinateSpace::Before)?;
        validate_range(after, &change.new_range, TextCoordinateSpace::After)?;
        if change.old_range.start < previous_old_end
            || change.new_range.start < previous_new_end
            || previous_old_start.is_some_and(|start| change.old_range.start <= start)
            || previous_new_start.is_some_and(|start| change.new_range.start <= start)
        {
            return Err(TextChangeError::UnorderedOrOverlapping { index });
        }
        previous_old_end = change.old_range.end;
        previous_new_end = change.new_range.end;
        previous_old_start = Some(change.old_range.start);
        previous_new_start = Some(change.new_range.start);
    }

    let mut old_cursor = 0;
    let mut new_cursor = 0;
    for (index, change) in changes.iter().enumerate() {
        if before.get(old_cursor..change.old_range.start)
            != after.get(new_cursor..change.new_range.start)
        {
            return Err(TextChangeError::InconsistentCoordinates { index });
        }
        if before.get(change.old_range.clone()) == after.get(change.new_range.clone()) {
            return Err(TextChangeError::UnchangedEntry { index });
        }
        old_cursor = change.old_range.end;
        new_cursor = change.new_range.end;
    }
    if before.get(old_cursor..) != after.get(new_cursor..) {
        return Err(TextChangeError::InconsistentCoordinates {
            index: changes.len(),
        });
    }
    Ok(())
}

fn validate_range(
    document: &str,
    range: &Range<usize>,
    space: TextCoordinateSpace,
) -> Result<(), TextChangeError> {
    if range.start > range.end || range.end > document.len() {
        return Err(TextChangeError::InvalidRange {
            space,
            start: range.start,
            end: range.end,
            len: document.len(),
        });
    }
    for offset in [range.start, range.end] {
        if !document.is_char_boundary(offset) {
            return Err(TextChangeError::InvalidUtf8Boundary { space, offset });
        }
    }
    Ok(())
}
