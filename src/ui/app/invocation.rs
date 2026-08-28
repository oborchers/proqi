//! Bounded authoring completion for discovered local definitions.

use std::ops::Range;

use crate::{
    application::Effect,
    ports::{
        editor::EditCommand,
        environment::{Clock, IdGenerator},
        invocation::{
            InvocationCatalogError, InvocationDiscovery, InvocationDiscoveryRequest,
            InvocationEntry, InvocationForm,
        },
        text_layout::{byte_for_position, position_for_byte},
    },
    ui::{PointerKind, UiInput, UiKey},
};

use super::BoardApp;

#[path = "invocation/builtins.rs"]
pub(in crate::ui::app) mod builtins;
#[path = "invocation/compatibility.rs"]
mod compatibility;
#[path = "invocation/highlight.rs"]
mod highlight;
#[path = "invocation/view.rs"]
mod view;

pub(in crate::ui) use view::InvocationChoiceView;

const MAX_RESULTS: usize = 20;

pub(super) struct InvocationPopup {
    query: String,
    range: Option<Range<usize>>,
    manual: bool,
    selected: usize,
    scroll: usize,
}

#[derive(Clone)]
struct Choice {
    token: String,
    qualifier: String,
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
    pub fn complete_invocation_discovery(
        &mut self,
        result: Result<InvocationDiscovery, InvocationCatalogError>,
    ) {
        let Ok(discovery) = result else {
            self.set_warning("invocation refresh exceeded its bounded root budget");
            return;
        };
        if discovery.generation != self.invocation_generation
            || discovery.cwd != self.invocation_cwd
        {
            return;
        }
        self.invocation_global = discovery.global;
        self.invocation_project = discovery.project;
        self.refresh_invocation_popup();
    }

    /// Replace only project discovery when the runtime cwd changes.
    pub fn set_invocation_cwd(&mut self, cwd: std::path::PathBuf) -> Vec<Effect> {
        if cwd != self.invocation_cwd {
            self.invocation_cwd = cwd;
            self.invocation_project.clear();
            self.invocation_popup = None;
        }
        self.refresh_invocations()
    }

    pub(super) fn open_invocation_picker(&mut self) {
        if !matches!(
            self.interaction_mode(),
            crate::application::InteractionMode::Edit { .. }
        ) {
            self.enter_edit();
        }
        if matches!(
            self.interaction_mode(),
            crate::application::InteractionMode::Edit { .. }
        ) {
            self.invocation_popup = Some(InvocationPopup {
                query: String::new(),
                range: None,
                manual: true,
                selected: 0,
                scroll: 0,
            });
        } else {
            self.set_warning("focus a thought before inserting an invocation");
        }
    }

    pub(super) fn refresh_invocation_popup(&mut self) {
        if self
            .invocation_popup
            .as_ref()
            .is_some_and(|popup| popup.manual)
        {
            return;
        }
        self.invocation_popup = self.active_invocation_token().and_then(|(query, range)| {
            let popup = InvocationPopup {
                query,
                range: Some(range),
                manual: false,
                selected: 0,
                scroll: 0,
            };
            (!self.invocation_choices(&popup).is_empty()).then_some(popup)
        });
    }

    pub(super) fn invocation_view(&self) -> Option<(String, Vec<InvocationChoiceView>, usize)> {
        let popup = self.invocation_popup.as_ref()?;
        let choices = self.invocation_choices(popup);
        Some((
            popup.query.clone(),
            choices
                .into_iter()
                .skip(popup.scroll)
                .map(|choice| InvocationChoiceView {
                    token: choice.token,
                    qualifier: choice.qualifier,
                })
                .collect(),
            popup.selected.saturating_sub(popup.scroll),
        ))
    }

    pub(super) fn invocation_match_count(&self) -> usize {
        self.invocation_popup
            .as_ref()
            .map_or(0, |popup| self.invocation_choices(popup).len())
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
            UiInput::Key(UiKey::Escape) => self.invocation_popup = None,
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
            UiInput::Pointer(pointer) => {
                if matches!(
                    pointer.kind,
                    PointerKind::Down(crate::ui::PointerButton::Left)
                ) {
                    return self.handle_pointer(*pointer, ids, clock);
                }
            }
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
            UiInput::Key(UiKey::Backspace) => self.pop_manual_query(),
            UiInput::Paste(value) => self.extend_manual_query(value),
            UiInput::PasteAnnotated(payload) => self.extend_manual_query(&payload.content),
            UiInput::Resize { .. }
            | UiInput::HostFocusGained
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
        let effects = match input {
            UiInput::Key(key) => self.handle_edit_key(*key, ids, clock),
            UiInput::Paste(value) => {
                self.paste_payload(crate::ui::PastePayload::text(value.clone()), ids, clock)
            }
            UiInput::PasteAnnotated(payload) => self.paste_payload(payload.clone(), ids, clock),
            UiInput::Resize { .. } | UiInput::HostFocusGained | UiInput::Pointer(_) => Vec::new(),
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
            .and_then(|popup| self.invocation_choices(popup).get(index).cloned());
        let range = self
            .invocation_popup
            .as_ref()
            .and_then(|popup| popup.range.clone())
            .or_else(|| {
                self.editor_snapshot().map(|snapshot| {
                    let byte = byte_for_position(&snapshot.content, snapshot.cursor);
                    byte..byte
                })
            });
        let (Some(choice), Some(mut range)) = (selected, range) else {
            return;
        };
        let Some(snapshot) = self.editor_snapshot() else {
            return;
        };
        if snapshot.content.get(range.clone()).is_none() {
            return;
        }
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
        self.apply_edit(EditCommand::Paste(format!("{} ", choice.token)));
        self.invocation_popup = None;
    }

    fn move_invocation(&mut self, delta: isize) {
        let count = self.invocation_match_count();
        let visible = self
            .layout
            .as_ref()
            .and_then(|layout| layout.overlay.as_ref())
            .map_or(1, |overlay| overlay.items.len().max(1));
        let Some(popup) = &mut self.invocation_popup else {
            return;
        };
        popup.selected = popup
            .selected
            .saturating_add_signed(delta)
            .min(count.saturating_sub(1));
        if popup.selected < popup.scroll {
            popup.scroll = popup.selected;
        } else if popup.selected >= popup.scroll.saturating_add(visible) {
            popup.scroll = popup.selected + 1 - visible;
        }
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

    fn invocation_choices(&self, popup: &InvocationPopup) -> Vec<Choice> {
        let query = popup.query.to_lowercase();
        let built_ins = builtins::choices(self, popup);
        let starts_prompt = builtins::starts_prompt(self, popup);
        let mut candidates = self
            .invocation_project
            .iter()
            .chain(&self.invocation_global)
            .flat_map(|entry| entry.forms.iter().map(move |form| (entry, form)))
            .filter(|(_, form)| compatibility::supports_form(self, form))
            .filter(|(_, form)| !builtins::is_shared_starter(&form.token) || starts_prompt)
            .filter(|(_, form)| {
                !built_ins
                    .iter()
                    .any(|built_in| built_in.token == form.token)
            })
            .filter(|(entry, form)| choice_matches(entry, form, popup.manual, &query))
            .collect::<Vec<_>>();
        candidates.sort_by(|(left_entry, left_form), (right_entry, right_form)| {
            left_form
                .precedence
                .cmp(&right_form.precedence)
                .then_with(|| left_form.token.cmp(&right_form.token))
                .then_with(|| left_entry.kind.cmp(&right_entry.kind))
                .then_with(|| left_entry.source.cmp(&right_entry.source))
                .then_with(|| left_entry.canonical_path.cmp(&right_entry.canonical_path))
        });
        candidates.truncate(MAX_RESULTS.saturating_sub(built_ins.len()));
        let visible = candidates.clone();
        built_ins
            .into_iter()
            .chain(candidates.drain(..).map(|(entry, form)| {
                let duplicate_token = visible
                    .iter()
                    .filter(|(_, visible_form)| visible_form.token == form.token)
                    .count()
                    > 1;
                Choice {
                    token: form.token.clone(),
                    qualifier: choice_qualifier(entry, form, duplicate_token),
                }
            }))
            .collect()
    }
}

fn choice_matches(
    entry: &InvocationEntry,
    form: &InvocationForm,
    manual: bool,
    query: &str,
) -> bool {
    if !manual {
        return form.token.to_lowercase().starts_with(query);
    }
    form.token.to_lowercase().contains(query)
        || entry.name.to_lowercase().contains(query)
        || entry
            .description
            .as_ref()
            .is_some_and(|description| description.to_lowercase().contains(query))
}

fn choice_qualifier(entry: &InvocationEntry, form: &InvocationForm, show_source: bool) -> String {
    let base = format!("{} {}", entry.scope.label(), entry.kind.label());
    if show_source {
        let harness = match form.harness {
            crate::ports::invocation::InvocationHarness::ClaudeCode => "Claude",
            harness => harness.label(),
        };
        format!("{base} · {harness}")
    } else {
        base
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
        character.is_alphanumeric() || matches!(character, '-' | '_' | ':' | '/' | '.')
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
#[path = "invocation/tests.rs"]
mod tests;
