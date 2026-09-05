//! Bounded authoring completion for discovered local definitions.

use std::ops::Range;

use unicode_normalization::char::is_combining_mark;

use crate::{
    application::Effect,
    ports::{
        editor::EditCommand,
        environment::{Clock, IdGenerator},
        invocation::{InvocationDiscovery, InvocationDiscoveryRequest},
        text_layout::{byte_for_position, position_for_byte},
    },
    ui::{PointerKind, UiInput, UiKey},
};

use super::BoardApp;

#[path = "invocation/builtins.rs"]
pub(in crate::ui::app) mod builtins;
#[path = "invocation/choices.rs"]
mod choices;
#[path = "invocation/compatibility.rs"]
mod compatibility;
#[path = "invocation/highlight.rs"]
mod highlight;
#[path = "invocation/matcher.rs"]
mod matcher;
#[path = "invocation/navigation.rs"]
mod navigation;
#[path = "invocation/reference.rs"]
mod reference;
#[path = "invocation/view.rs"]
mod view;

use view::Choice;
pub(in crate::ui) use view::InvocationChoiceView;

pub(super) struct InvocationPopup {
    query: String,
    range: Option<Range<usize>>,
    manual: bool,
    selected: usize,
    scroll: usize,
    choices: Vec<Choice>,
}

impl BoardApp {
    /// Request one generation-tagged catalog refresh.
    pub fn refresh_invocations(&mut self) -> Vec<Effect> {
        self.invocation_generation = self.invocation_generation.wrapping_add(1);
        vec![Effect::DiscoverInvocations(InvocationDiscoveryRequest {
            generation: self.invocation_generation,
            cwd: self.invocation_cwd.clone(),
        })]
    }

    /// Apply only the newest discovery for the current cwd.
    pub fn complete_invocation_discovery(&mut self, discovery: InvocationDiscovery) -> bool {
        if discovery.generation != self.invocation_generation
            || discovery.cwd != self.invocation_cwd
        {
            return false;
        }
        self.invocation_global = discovery.global;
        self.invocation_project = discovery.project;
        self.invocation_completeness
            .set_filesystem(discovery.completeness);
        self.refresh_invocation_popup();
        self.clamp_invocation_popup();
        true
    }

    /// Replace only project discovery when the runtime cwd changes.
    pub fn set_invocation_cwd(&mut self, cwd: std::path::PathBuf) -> Vec<Effect> {
        if cwd != self.invocation_cwd {
            self.invocation_cwd = cwd;
            self.invocation_project.clear();
            self.close_invocation_picker();
        }
        self.refresh_invocations()
    }

    pub(super) fn refresh_invocation_popup(&mut self) {
        if self
            .invocation_popup
            .as_ref()
            .is_some_and(|popup| popup.manual)
        {
            self.rebuild_invocation_choices();
            return;
        }
        self.invocation_popup = self.active_invocation_token().and_then(|(query, range)| {
            let mut popup = InvocationPopup {
                query,
                range: Some(range),
                manual: false,
                selected: 0,
                scroll: 0,
                choices: Vec::new(),
            };
            popup.choices = choices::build(self, &popup);
            (!popup.choices.is_empty()).then_some(popup)
        });
    }

    pub(super) fn rebuild_invocation_choices(&mut self) {
        let Some(mut popup) = self.invocation_popup.take() else {
            return;
        };
        popup.choices = choices::build(self, &popup);
        self.invocation_popup = Some(popup);
    }

    pub(super) fn invocation_view(&self) -> Option<(String, Vec<InvocationChoiceView>, usize)> {
        let popup = self.invocation_popup.as_ref()?;
        let mut previous_group = None;
        Some((
            popup.query.clone(),
            popup
                .choices
                .iter()
                .skip(popup.scroll)
                .map(|choice| {
                    let group = choice.group.clone().and_then(|group| {
                        let begins_group = previous_group.as_deref() != Some(group.as_str());
                        previous_group = Some(group.clone());
                        begins_group.then_some(group)
                    });
                    InvocationChoiceView {
                        token: choice.token.clone(),
                        qualifier: choice.qualifier.clone(),
                        qualifier_fallbacks: choice.qualifier_fallbacks.clone(),
                        group,
                    }
                })
                .collect(),
            popup.selected.saturating_sub(popup.scroll),
        ))
    }

    pub(super) fn invocation_match_count(&self) -> usize {
        self.invocation_popup
            .as_ref()
            .map_or(0, |popup| popup.choices.len())
    }

    pub(in crate::ui) fn invocation_notice(&self) -> Option<&'static str> {
        if !self.invocation_completeness.combined().is_complete() {
            Some(" incomplete results, refine query ")
        } else if self.invocation_match_count() > 20 {
            Some(" more results exist, refine query ")
        } else {
            None
        }
    }

    pub(super) fn invocation_overflow(&self, visible: usize) -> (bool, bool) {
        self.invocation_popup
            .as_ref()
            .map_or((false, false), |popup| {
                (
                    popup.scroll > 0,
                    popup.scroll.saturating_add(visible) < popup.choices.len(),
                )
            })
    }

    pub(super) fn invocation_query_cursor(&self) -> Option<usize> {
        self.invocation_popup
            .as_ref()
            .map(|popup| popup.query.len())
    }

    pub(super) fn handle_invocation_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match input {
            UiInput::Key(UiKey::Escape) => self.close_invocation_picker(),
            UiInput::Key(UiKey::Enter | UiKey::Tab) => {
                let selected = self
                    .invocation_popup
                    .as_ref()
                    .map_or(0, |popup| popup.selected);
                self.execute_invocation_index(selected);
            }
            UiInput::Key(
                UiKey::PickerPrevious
                | UiKey::Move {
                    movement: crate::ports::editor::CursorMovement::VisualUp,
                    ..
                },
            ) => self.move_invocation(-1),
            UiInput::Key(
                UiKey::PickerNext
                | UiKey::Move {
                    movement: crate::ports::editor::CursorMovement::VisualDown,
                    ..
                },
            ) => self.move_invocation(1),
            UiInput::Key(UiKey::FastNavigation { direction, .. }) => {
                self.move_invocation(direction.delta());
            }
            UiInput::Pointer(pointer) => match pointer.kind {
                PointerKind::ScrollUp => self.move_invocation(-1),
                PointerKind::ScrollDown => self.move_invocation(1),
                PointerKind::Down(crate::ui::PointerButton::Left) => {
                    return self.handle_pointer(*pointer, ids, clock);
                }
                _ => {}
            },
            _ if self
                .invocation_popup
                .as_ref()
                .is_some_and(|popup| popup.manual) =>
            {
                return self.handle_manual_input(input);
            }
            _ => return self.handle_automatic_input(input, ids, clock),
        }
        Vec::new()
    }

    fn handle_manual_input(&mut self, input: &UiInput) -> Vec<Effect> {
        match input {
            UiInput::Key(UiKey::Character(character)) if !character.is_control() => {
                self.update_manual_query(|query| query.push(*character));
            }
            UiInput::Key(UiKey::UnmodifiedSpace) => {
                self.update_manual_query(|query| query.push(' '));
            }
            UiInput::Key(UiKey::Backspace) => self.pop_manual_query(),
            UiInput::Paste(value) => self.extend_manual_query(value),
            UiInput::PasteAnnotated(payload) => self.extend_manual_query(&payload.content),
            UiInput::Resize { .. }
            | UiInput::HostFocusGained
            | UiInput::HostFocusLost
            | UiInput::Key(_)
            | UiInput::Pointer(_) => {}
        }
        Vec::new()
    }

    fn handle_automatic_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if matches!(
            input,
            UiInput::Resize { .. } | UiInput::HostFocusGained | UiInput::HostFocusLost
        ) {
            return Vec::new();
        }
        let effects = match input {
            UiInput::Key(key) => self.handle_edit_key(*key, ids, clock),
            UiInput::Paste(value) => {
                self.paste_payload(crate::ui::PastePayload::text(value.clone()), ids, clock)
            }
            UiInput::PasteAnnotated(payload) => self.paste_payload(payload.clone(), ids, clock),
            UiInput::Pointer(_)
            | UiInput::Resize { .. }
            | UiInput::HostFocusGained
            | UiInput::HostFocusLost => Vec::new(),
        };
        self.refresh_invocation_popup();
        effects
    }

    fn pop_manual_query(&mut self) {
        self.update_manual_query(|query| {
            query.pop();
        });
    }

    fn extend_manual_query(&mut self, value: &str) {
        self.update_manual_query(|query| {
            query.extend(value.chars().filter(|character| !character.is_control()));
        });
    }

    pub(super) fn execute_invocation_visible_index(&mut self, index: usize) -> bool {
        let Some(popup) = self.invocation_popup.as_ref() else {
            return false;
        };
        let absolute = popup.scroll.saturating_add(index);
        self.execute_invocation_index(absolute);
        true
    }

    fn execute_invocation_index(&mut self, index: usize) {
        let selected = self
            .invocation_popup
            .as_ref()
            .and_then(|popup| popup.choices.get(index).cloned());
        let Some(choice) = selected else {
            return;
        };
        let Some(snapshot) = self.editor_snapshot() else {
            return;
        };
        let mut range = self
            .invocation_popup
            .as_ref()
            .and_then(|popup| popup.range.clone())
            .or_else(|| {
                snapshot.selection.map(|selection| {
                    byte_for_position(&snapshot.content, selection.start)
                        ..byte_for_position(&snapshot.content, selection.end)
                })
            })
            .unwrap_or_else(|| {
                let byte = byte_for_position(&snapshot.content, snapshot.cursor);
                byte..byte
            });
        if snapshot.content.get(range.clone()).is_none() {
            return;
        }
        let insertion = reference::insertion_payload(&choice, &snapshot.content, &range);
        if snapshot.content.as_bytes().get(range.end) == Some(&b' ') {
            range.end = range.end.saturating_add(1);
        }
        self.apply_edit(EditCommand::SetCursor {
            position: position_for_byte(&snapshot.content, range.start),
            extend_selection: false,
        });
        self.apply_edit(EditCommand::SetCursor {
            position: position_for_byte(&snapshot.content, range.end),
            extend_selection: true,
        });
        let (content, annotations, _, _) = insertion.into_parts();
        self.apply_annotated_edit(EditCommand::Paste(content), &annotations);
        self.close_invocation_picker();
    }

    fn update_manual_query(&mut self, update: impl FnOnce(&mut String)) {
        if let Some(popup) = &mut self.invocation_popup {
            update(&mut popup.query);
            if let Some((byte, _)) = popup.query.char_indices().nth(128) {
                popup.query.truncate(byte);
            }
            popup.selected = 0;
            popup.scroll = 0;
        }
        self.rebuild_invocation_choices();
    }

    fn active_invocation_token(&self) -> Option<(String, Range<usize>)> {
        let snapshot = self.editor_snapshot()?;
        if snapshot.selection.is_some() {
            return None;
        }
        let cursor = byte_for_position(&snapshot.content, snapshot.cursor);
        if inside_code_fence(&snapshot.content[..cursor]) {
            return None;
        }
        let line_start = snapshot.content[..cursor]
            .rfind('\n')
            .map_or(0, |byte| byte + 1);
        let start = snapshot.content[line_start..cursor]
            .char_indices()
            .rev()
            .find(|(_, character)| character.is_whitespace())
            .map_or(line_start, |(byte, character)| {
                line_start + byte + character.len_utf8()
            });
        let query = snapshot.content.get(start..cursor)?;
        if !plausible(query) {
            return None;
        }
        let end = snapshot.content[cursor..]
            .find(|character: char| character.is_whitespace())
            .map_or(snapshot.content.len(), |byte| cursor + byte);
        Some((query.to_owned(), start..end))
    }
}

fn plausible(query: &str) -> bool {
    let mut characters = query.chars();
    let Some(sigil @ ('$' | '/' | '@')) = characters.next() else {
        return false;
    };
    let body = characters.collect::<String>();
    if body.is_empty() || body.chars().count() > 96 {
        return false;
    }
    if sigil == '$'
        && (body.starts_with(|character: char| {
            character.is_ascii_digit() || matches!(character, '-' | '_')
        }) || body
            .chars()
            .all(|character| character.is_ascii_uppercase() || character == '_'))
    {
        return false;
    }
    body.chars().all(|character| {
        character.is_alphanumeric()
            || is_combining_mark(character)
            || matches!(character, '-' | '_' | ':' | '/' | '.')
    })
}

fn inside_code_fence(prefix: &str) -> bool {
    prefix
        .lines()
        .filter(|line| line.trim_start().starts_with("```"))
        .count()
        % 2
        == 1
}

#[cfg(test)]
#[path = "invocation/discovery_tests.rs"]
mod discovery_tests;
#[cfg(test)]
#[path = "invocation/paging_tests.rs"]
mod paging_tests;
#[cfg(test)]
#[path = "invocation/ranking_tests.rs"]
mod ranking_tests;
#[cfg(test)]
#[path = "invocation/tests.rs"]
mod tests;
