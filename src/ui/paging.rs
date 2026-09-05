//! Canonical fast-navigation semantics shared by editors and modal surfaces.

use crate::ports::editor::{CursorMovement, FAST_NAVIGATION_ROWS};

/// Exact number of visual rows or eligible entries in one fast movement.
pub(crate) const FAST_NAVIGATION_STEP: usize = FAST_NAVIGATION_ROWS;

/// Direction of one normalized fast-navigation intention.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FastNavigation {
    /// Move toward earlier content.
    Previous,
    /// Move toward later content.
    Next,
}

impl FastNavigation {
    /// Signed five-step delta for selectable entries or visible content rows.
    pub(crate) const fn delta(self) -> isize {
        match self {
            Self::Previous => -FAST_NAVIGATION_STEP.cast_signed(),
            Self::Next => FAST_NAVIGATION_STEP.cast_signed(),
        }
    }

    /// Existing editor movement that preserves the preferred terminal-cell column.
    pub(crate) const fn editor_movement(self) -> CursorMovement {
        match self {
            Self::Previous => CursorMovement::VisualJumpUp,
            Self::Next => CursorMovement::VisualJumpDown,
        }
    }

    /// One-row Board movement retained for Alt and shifted-Alt compatibility.
    pub(crate) const fn board_movement(self) -> CursorMovement {
        match self {
            Self::Previous => CursorMovement::VisualUp,
            Self::Next => CursorMovement::VisualDown,
        }
    }

    /// Move and clamp one selected eligible-entry index.
    pub(crate) fn move_index(self, selected: usize, count: usize) -> usize {
        selected
            .saturating_add_signed(self.delta())
            .min(count.saturating_sub(1))
    }

    /// Move and clamp one scroll-only row offset.
    pub(crate) fn move_scroll(self, scroll: usize, maximum: usize) -> usize {
        scroll.saturating_add_signed(self.delta()).min(maximum)
    }
}

/// Canonical contextual-help key for fast navigation.
pub(crate) const FAST_NAVIGATION_SHORTCUT_KEY: &str = "Alt+↑/↓";

/// Canonical full spelling pair retained in public controls documentation.
#[cfg(test)]
pub(crate) const FAST_NAVIGATION_README_LABEL: &str = "Alt+↑ / ↓ or Page Up / Page Down";

/// Canonical command-discovery label for one fast movement.
#[cfg(test)]
pub(crate) const fn command_label(direction: FastNavigation) -> &'static str {
    match direction {
        FastNavigation::Previous => "Jump cursor up 5 visual rows (Alt+↑ or Page Up)",
        FastNavigation::Next => "Jump cursor down 5 visual rows (Alt+↓ or Page Down)",
    }
}

/// Clamp a bounded selectable viewport so its selected eligible entry is visible.
pub(crate) fn first_visible(selected: usize, current: usize, visible: usize) -> usize {
    let visible = visible.max(1);
    if selected < current {
        selected
    } else if selected >= current.saturating_add(visible) {
        selected.saturating_add(1).saturating_sub(visible)
    } else {
        current
    }
}

#[cfg(test)]
mod tests {
    use super::{FAST_NAVIGATION_STEP, FastNavigation};

    #[test]
    fn exact_five_step_clamps_empty_short_exact_and_long_inventories() {
        assert_eq!(FAST_NAVIGATION_STEP, 5);
        for (count, start, direction, expected) in [
            (0, 0, FastNavigation::Next, 0),
            (1, 0, FastNavigation::Next, 0),
            (4, 0, FastNavigation::Next, 3),
            (5, 0, FastNavigation::Next, 4),
            (12, 0, FastNavigation::Next, 5),
            (12, 11, FastNavigation::Previous, 6),
            (12, 2, FastNavigation::Previous, 0),
            (12, 10, FastNavigation::Next, 11),
        ] {
            assert_eq!(direction.move_index(start, count), expected);
        }
    }

    #[test]
    fn selected_entry_stays_visible_at_both_viewport_edges() {
        assert_eq!(super::first_visible(2, 4, 3), 2);
        assert_eq!(super::first_visible(8, 4, 3), 6);
        assert_eq!(super::first_visible(5, 4, 3), 4);
        assert_eq!(super::first_visible(5, 4, 0), 5);
    }

    #[test]
    fn public_controls_document_both_canonical_spelling_families() {
        let readme = include_str!("../../README.md");
        assert!(
            readme
                .replace('`', "")
                .contains(super::FAST_NAVIGATION_README_LABEL)
        );
        for direction in [FastNavigation::Previous, FastNavigation::Next] {
            let label = super::command_label(direction);
            assert!(label.contains("Alt+"));
            assert!(label.contains("Page "));
        }
    }
}
