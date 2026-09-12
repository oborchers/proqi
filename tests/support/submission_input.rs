//! Logical Control submission input shared only by submission behavior tests.

use proqi::ui::{KeyStroke, LogicalKey, LogicalModifiers, UiInput};

pub(crate) fn control_submit(keep: bool) -> UiInput {
    let modifiers = if keep {
        LogicalModifiers::CONTROL.union(LogicalModifiers::SHIFT)
    } else {
        LogicalModifiers::CONTROL
    };
    UiInput::KeyStroke(KeyStroke::press(LogicalKey::Enter).with_modifiers(modifiers))
}
