//! Footer labels projected from descriptor-owned metadata.

use crate::{
    application::InteractionMode,
    ui::{KeyBindings, ShortcutActionId as Action},
};

use super::{board_control_label, canonical_label, primary_label};

pub(crate) struct FooterProjection {
    pub(crate) key: String,
    pub(crate) text: &'static str,
    pub(crate) minimum_width: u16,
}

pub(crate) fn footer_projection(
    action: Action,
    compact: bool,
    mode: InteractionMode,
    keys: &KeyBindings,
) -> Option<FooterProjection> {
    let metadata = crate::ui::shortcut_registry::inventory::metadata::footer_metadata(action)?;
    Some(FooterProjection {
        key: footer_key(action, compact, mode, keys),
        text: if compact {
            metadata.compact_text
        } else {
            metadata.text
        },
        minimum_width: if compact {
            metadata.compact_minimum_width
        } else {
            metadata.minimum_width
        },
    })
}

fn footer_key(action: Action, compact: bool, mode: InteractionMode, keys: &KeyBindings) -> String {
    let editor_mode = matches!(
        mode,
        InteractionMode::Compose | InteractionMode::Edit { .. }
    );
    match action {
        Action::New => crate::ui::settings::key_label(keys.new),
        Action::Copy | Action::Cut | Action::Undo => {
            if editor_mode {
                canonical_label(action)
            } else {
                board_control_label(action, keys, compact)
            }
        }
        Action::Delete => keys.delete_label(),
        Action::Select => crate::ui::settings::key_label(keys.select),
        Action::OpenSearch => crate::ui::settings::key_label(keys.search),
        Action::OpenCommands => crate::ui::settings::key_label(keys.commands),
        Action::Help => crate::ui::settings::key_label(keys.help),
        Action::Quit => crate::ui::settings::key_label(keys.quit),
        Action::Close => "Esc".to_owned(),
        Action::RetryStorage => "r".to_owned(),
        Action::ExportRecovery => "w".to_owned(),
        Action::SubmitRemove if matches!(mode, InteractionMode::Edit { .. }) => {
            primary_label(action)
        }
        Action::SubmitKeep if matches!(mode, InteractionMode::Edit { .. }) => {
            canonical_label(action)
        }
        Action::SubmitRemove => crate::ui::settings::key_label(keys.submit_remove),
        Action::SubmitKeep => crate::ui::settings::key_label(keys.submit_keep),
        _ => canonical_label(action),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_and_full_copy_share_descriptor_copy_and_measurement() {
        let keys = KeyBindings::default();
        let full = footer_projection(Action::Copy, false, InteractionMode::Board, &keys)
            .expect("copy footer");
        let compact = footer_projection(Action::Copy, true, InteractionMode::Board, &keys)
            .expect("copy footer");
        assert_eq!(full.text, " Copy");
        assert_eq!(compact.text, full.text);
        assert_eq!(full.minimum_width, 7);
        assert_eq!(compact.minimum_width, 7);
        assert_eq!(compact.key, keys.copy.to_string());
    }
}
