//! Contextual Help metadata for terminal-safe Board boundary actions.

use super::{Action, help};
use crate::ui::shortcut_registry::model::{HelpAvailability, HelpMetadata, HelpSurface};

pub(super) const HELP: &[(Action, HelpMetadata)] = &[
    (
        Action::InsertAbove,
        help(
            HelpSurface::Board,
            28,
            "Insert above",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::InsertBelow,
        help(
            HelpSurface::Board,
            29,
            "Insert below",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::FocusFirst,
        help(
            HelpSurface::Board,
            30,
            "First thought",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::FocusLast,
        help(
            HelpSurface::Board,
            31,
            "Last thought",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::ExtendFirst,
        help(
            HelpSurface::Board,
            32,
            "Extend to first",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::ExtendLast,
        help(
            HelpSurface::Board,
            33,
            "Extend to last",
            HelpAvailability::Always,
        ),
    ),
];
