//! Typed content-free inspection of the active registry resolution.

pub(in crate::ui::shortcut_registry) mod intention;

use super::{
    ShortcutActionId, ShortcutContext, ShortcutContextStack, ShortcutPlatform, ShortcutRegistry,
};
use crate::ui::{KeyPhase, KeyStroke, LogicalKey, LogicalKeyState, LogicalModifiers, UiKey};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShortcutClassification {
    Resolved,
    ReleaseIgnored,
    NoOp,
    ReservedLiteral,
    Unbound,
}

impl ShortcutClassification {
    pub(crate) const fn diagnostics_id(self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            Self::ReleaseIgnored => "release_ignored",
            Self::NoOp => "no_op",
            Self::ReservedLiteral => "reserved_literal",
            Self::Unbound => "unbound",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShortcutInspection {
    pub(crate) stroke: KeyStroke,
    pub(crate) platform: ShortcutPlatform,
    pub(crate) context_stack: Vec<ShortcutContext>,
    pub(crate) active_context: Option<ShortcutContext>,
    pub(crate) classification: ShortcutClassification,
    pub(crate) action: Option<ShortcutActionId>,
    pub(crate) intention: Option<UiKey>,
}

pub(crate) struct ShortcutStrokeInspection {
    pub(crate) key: String,
    pub(crate) modifiers: Vec<&'static str>,
    pub(crate) state: Vec<&'static str>,
    pub(crate) phase: &'static str,
}

impl ShortcutInspection {
    pub(crate) fn capture_cancelled(&self) -> bool {
        self.stroke.key == LogicalKey::Escape && self.stroke.phase != KeyPhase::Release
    }

    pub(crate) fn intention_name(&self) -> Option<String> {
        self.intention.map(intention::name)
    }

    pub(crate) const fn classification_id(&self) -> &'static str {
        self.classification.diagnostics_id()
    }

    pub(crate) fn stroke_inspection(&self) -> ShortcutStrokeInspection {
        ShortcutStrokeInspection {
            key: key_name(self.stroke.key),
            modifiers: modifier_names(self.stroke.modifiers),
            state: [
                (LogicalKeyState::KEYPAD, "Keypad"),
                (LogicalKeyState::CAPS_LOCK, "CapsLock"),
                (LogicalKeyState::NUM_LOCK, "NumLock"),
            ]
            .into_iter()
            .filter_map(|(flag, name)| self.stroke.state.contains(flag).then_some(name))
            .collect(),
            phase: match self.stroke.phase {
                KeyPhase::Press => "press",
                KeyPhase::Repeat => "repeat",
                KeyPhase::Release => "release",
            },
        }
    }

    pub(crate) const fn platform_id(&self) -> &'static str {
        match self.platform {
            ShortcutPlatform::MacOs => "macos",
            ShortcutPlatform::Portable => "portable",
        }
    }

    pub(crate) fn primary_names(&self) -> Vec<&'static str> {
        self.platform
            .primary_modifiers()
            .iter()
            .flat_map(|modifiers| modifier_names(*modifiers))
            .collect()
    }

    pub(crate) fn binding_identity(&self) -> Option<String> {
        self.action
            .zip(self.active_context)
            .map(|(action, context)| {
                format!(
                    "{}:{}:{}:{}",
                    context.configuration_id(),
                    action.diagnostics_id(),
                    key_name(self.stroke.key),
                    modifier_names(self.stroke.modifiers).join("+")
                )
            })
    }
}

impl ShortcutRegistry {
    pub(crate) fn inspect(
        &self,
        contexts: &ShortcutContextStack,
        stroke: KeyStroke,
    ) -> ShortcutInspection {
        let resolved = self.dispatch(contexts, stroke);
        let action = resolved.and_then(|value| value.action);
        let active_context = contexts.active();
        let classification = if stroke.phase == KeyPhase::Release {
            ShortcutClassification::ReleaseIgnored
        } else if action.is_some_and(|action| known_no_op(active_context, action)) {
            ShortcutClassification::NoOp
        } else if action.is_some() {
            ShortcutClassification::Resolved
        } else if matches!(stroke.key, LogicalKey::Character(character) if !character.is_control())
            && super::validation::reserves_printable(stroke.modifiers)
            && active_context.is_some_and(super::inventory::bindings::vocabulary::is_text_context)
        {
            ShortcutClassification::ReservedLiteral
        } else {
            ShortcutClassification::Unbound
        };
        ShortcutInspection {
            stroke,
            platform: self.platform(),
            context_stack: contexts.as_slice().to_vec(),
            active_context,
            classification,
            action,
            intention: action.zip(resolved).map(|(_, value)| value.intention),
        }
    }
}

fn key_name(key: LogicalKey) -> String {
    match key {
        LogicalKey::Character(character) => format!("U+{:04X}", u32::from(character)),
        key => super::contract::key_name(key),
    }
}

fn modifier_names(modifiers: LogicalModifiers) -> Vec<&'static str> {
    [
        (LogicalModifiers::CONTROL, "Control"),
        (LogicalModifiers::ALT, "Alt"),
        (LogicalModifiers::SHIFT, "Shift"),
        (LogicalModifiers::SUPER, "Super"),
        (LogicalModifiers::META, "Meta"),
        (LogicalModifiers::HYPER, "Hyper"),
    ]
    .into_iter()
    .filter_map(|(flag, name)| modifiers.contains(flag).then_some(name))
    .collect()
}

fn known_no_op(context: Option<ShortcutContext>, action: ShortcutActionId) -> bool {
    use ShortcutActionId as Action;
    use ShortcutContext as Context;
    match action {
        Action::Close => context == Some(Context::Recovery),
        Action::FastPrevious
        | Action::FastNext
        | Action::FastExtendPrevious
        | Action::FastExtendNext => matches!(
            context,
            Some(
                Context::Rename
                    | Context::BrowserRename
                    | Context::ExportPath
                    | Context::ExportReplace
                    | Context::Recovery
                    | Context::Direction
            )
        ),
        _ => false,
    }
}
