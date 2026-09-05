//! Registry contract tests split by responsibility.

mod dispatch;
mod edge_cases;
mod inventory;
mod platform;
mod validation;

use crate::ui::{KeyPhase, KeyStroke, LogicalKey, LogicalKeyState, LogicalModifiers};

fn stroke(key: LogicalKey, modifiers: LogicalModifiers) -> KeyStroke {
    KeyStroke {
        key,
        modifiers,
        phase: KeyPhase::Press,
        state: LogicalKeyState::NONE,
    }
}
