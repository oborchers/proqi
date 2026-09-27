//! Stable external names for logical keys and contextual action identities.

use super::model::ShortcutContext;
use crate::ui::{LogicalKey, LogicalMediaKey, LogicalModifierKey};

pub(super) const CONTEXT_NAMES: &[(ShortcutContext, &str)] = &[
    (ShortcutContext::Board, "board"),
    (ShortcutContext::Compose, "compose"),
    (ShortcutContext::Edit, "edit"),
    (ShortcutContext::Help, "help"),
    (ShortcutContext::Commands, "commands"),
    (ShortcutContext::Search, "search"),
    (ShortcutContext::Invocation, "invocation"),
    (ShortcutContext::InvocationQuery, "invocation_query"),
    (ShortcutContext::Transfer, "transfer"),
    (
        ShortcutContext::GlobalDeliveryQuery,
        "global_delivery_query",
    ),
    (
        ShortcutContext::GlobalDeliveryDisposition,
        "global_delivery_disposition",
    ),
    (ShortcutContext::Browser, "browser"),
    (ShortcutContext::BrowserQuery, "browser_query"),
    (ShortcutContext::Rename, "rename"),
    (ShortcutContext::BrowserRename, "browser_rename"),
    (ShortcutContext::ExportPath, "export_path"),
    (ShortcutContext::ExportReplace, "export_replace"),
    (ShortcutContext::Update, "update"),
    (ShortcutContext::Screenshot, "screenshot"),
    (ShortcutContext::Recovery, "recovery"),
    (ShortcutContext::Direction, "direction"),
    (ShortcutContext::ReleaseHighlights, "release_highlights"),
    (ShortcutContext::InsertionBoundary, "insertion_boundary"),
];

const NAMED_KEYS: &[(LogicalKey, &str)] = &[
    (LogicalKey::Character(' '), "Space"),
    (LogicalKey::Backspace, "Backspace"),
    (LogicalKey::Enter, "Enter"),
    (LogicalKey::Left, "Left"),
    (LogicalKey::Right, "Right"),
    (LogicalKey::Up, "Up"),
    (LogicalKey::Down, "Down"),
    (LogicalKey::Home, "Home"),
    (LogicalKey::End, "End"),
    (LogicalKey::PageUp, "PageUp"),
    (LogicalKey::PageDown, "PageDown"),
    (LogicalKey::Tab, "Tab"),
    (LogicalKey::BackTab, "BackTab"),
    (LogicalKey::Delete, "Delete"),
    (LogicalKey::Insert, "Insert"),
    (LogicalKey::Null, "Null"),
    (LogicalKey::Escape, "Escape"),
    (LogicalKey::CapsLock, "CapsLock"),
    (LogicalKey::ScrollLock, "ScrollLock"),
    (LogicalKey::NumLock, "NumLock"),
    (LogicalKey::PrintScreen, "PrintScreen"),
    (LogicalKey::Pause, "Pause"),
    (LogicalKey::Menu, "Menu"),
    (LogicalKey::KeypadBegin, "KeypadBegin"),
    (LogicalKey::Media(LogicalMediaKey::Play), "Media.Play"),
    (LogicalKey::Media(LogicalMediaKey::Pause), "Media.Pause"),
    (
        LogicalKey::Media(LogicalMediaKey::PlayPause),
        "Media.PlayPause",
    ),
    (LogicalKey::Media(LogicalMediaKey::Reverse), "Media.Reverse"),
    (LogicalKey::Media(LogicalMediaKey::Stop), "Media.Stop"),
    (
        LogicalKey::Media(LogicalMediaKey::FastForward),
        "Media.FastForward",
    ),
    (LogicalKey::Media(LogicalMediaKey::Rewind), "Media.Rewind"),
    (
        LogicalKey::Media(LogicalMediaKey::TrackNext),
        "Media.TrackNext",
    ),
    (
        LogicalKey::Media(LogicalMediaKey::TrackPrevious),
        "Media.TrackPrevious",
    ),
    (LogicalKey::Media(LogicalMediaKey::Record), "Media.Record"),
    (
        LogicalKey::Media(LogicalMediaKey::LowerVolume),
        "Media.LowerVolume",
    ),
    (
        LogicalKey::Media(LogicalMediaKey::RaiseVolume),
        "Media.RaiseVolume",
    ),
    (
        LogicalKey::Media(LogicalMediaKey::MuteVolume),
        "Media.MuteVolume",
    ),
    (
        LogicalKey::Modifier(LogicalModifierKey::LeftShift),
        "Modifier.LeftShift",
    ),
    (
        LogicalKey::Modifier(LogicalModifierKey::LeftControl),
        "Modifier.LeftControl",
    ),
    (
        LogicalKey::Modifier(LogicalModifierKey::LeftAlt),
        "Modifier.LeftAlt",
    ),
    (
        LogicalKey::Modifier(LogicalModifierKey::LeftSuper),
        "Modifier.LeftSuper",
    ),
    (
        LogicalKey::Modifier(LogicalModifierKey::LeftHyper),
        "Modifier.LeftHyper",
    ),
    (
        LogicalKey::Modifier(LogicalModifierKey::LeftMeta),
        "Modifier.LeftMeta",
    ),
    (
        LogicalKey::Modifier(LogicalModifierKey::RightShift),
        "Modifier.RightShift",
    ),
    (
        LogicalKey::Modifier(LogicalModifierKey::RightControl),
        "Modifier.RightControl",
    ),
    (
        LogicalKey::Modifier(LogicalModifierKey::RightAlt),
        "Modifier.RightAlt",
    ),
    (
        LogicalKey::Modifier(LogicalModifierKey::RightSuper),
        "Modifier.RightSuper",
    ),
    (
        LogicalKey::Modifier(LogicalModifierKey::RightHyper),
        "Modifier.RightHyper",
    ),
    (
        LogicalKey::Modifier(LogicalModifierKey::RightMeta),
        "Modifier.RightMeta",
    ),
    (
        LogicalKey::Modifier(LogicalModifierKey::IsoLevel3Shift),
        "Modifier.IsoLevel3Shift",
    ),
    (
        LogicalKey::Modifier(LogicalModifierKey::IsoLevel5Shift),
        "Modifier.IsoLevel5Shift",
    ),
];

impl ShortcutContext {
    pub(crate) fn parse_configuration_id(name: &str) -> Option<Self> {
        CONTEXT_NAMES
            .iter()
            .find_map(|(context, candidate)| (*candidate == name).then_some(*context))
    }

    /// Stable configuration and content-redacted diagnostic identity.
    #[must_use]
    pub fn configuration_id(self) -> &'static str {
        CONTEXT_NAMES
            .iter()
            .find_map(|(context, name)| (*context == self).then_some(*name))
            .unwrap_or("unknown")
    }
}

pub(super) fn parse_key(name: &str) -> Option<LogicalKey> {
    if let Some((key, _)) = NAMED_KEYS.iter().find(|(_, candidate)| *candidate == name) {
        return Some(*key);
    }
    if let Some(number) = name
        .strip_prefix('F')
        .and_then(|value| value.parse::<u8>().ok())
    {
        return ((1..=35).contains(&number) && name == format!("F{number}"))
            .then_some(LogicalKey::Function(number));
    }
    let mut characters = name.chars();
    let character = characters.next()?;
    (characters.next().is_none() && !character.is_control())
        .then_some(LogicalKey::Character(character))
}

pub(super) fn key_name(key: LogicalKey) -> String {
    if let Some((_, name)) = NAMED_KEYS.iter().find(|(candidate, _)| *candidate == key) {
        return (*name).to_owned();
    }
    match key {
        LogicalKey::Character(character) => character.to_string(),
        LogicalKey::Function(number) => format!("F{number}"),
        _ => "Unknown".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_key_and_context_has_one_round_tripping_contract_identity() {
        let mut names = std::collections::BTreeSet::new();
        for (key, name) in NAMED_KEYS {
            assert!(names.insert(*name));
            assert_eq!(parse_key(name), Some(*key));
            assert_eq!(key_name(*key), *name);
        }
        for number in 1..=35 {
            let key = LogicalKey::Function(number);
            assert_eq!(parse_key(&key_name(key)), Some(key));
        }
        let mut names = std::collections::BTreeSet::new();
        for (context, name) in CONTEXT_NAMES {
            assert!(names.insert(*name));
            assert_eq!(
                ShortcutContext::parse_configuration_id(name),
                Some(*context)
            );
            assert_eq!(context.configuration_id(), *name);
        }
    }
}
