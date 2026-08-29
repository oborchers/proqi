//! Searchable thought picker for the active board.

use unicode_segmentation::UnicodeSegmentation as _;

use crate::{
    application::{Action, Effect},
    domain::ThoughtId,
    ports::environment::{Clock, IdGenerator},
};

use super::{BoardApp, UiInput, UiKey, query::QueryEditor};

pub(super) struct SearchState {
    query: QueryEditor,
    selected: usize,
    scroll: usize,
}

impl SearchState {
    fn new() -> Self {
        Self {
            query: QueryEditor::default(),
            selected: 0,
            scroll: 0,
        }
    }

    pub(super) const fn query_cursor(&self) -> usize {
        self.query.cursor()
    }
}

impl BoardApp {
    pub(super) fn open_search(&mut self) {
        self.deactivate_range_latch();
        self.help = false;
        self.palette = None;
        self.search = Some(SearchState::new());
    }

    /// Current thought-search query, visible excerpts, and selected row.
    #[must_use]
    pub fn search_view(&self) -> Option<(String, Vec<String>, usize)> {
        let search = self.search.as_ref()?;
        let matches = self.search_matches();
        let entries = matches
            .iter()
            .skip(search.scroll)
            .filter_map(|id| self.state.board.thought(*id))
            .map(|thought| excerpt(&thought.content))
            .collect();
        Some((
            search.query.text().to_owned(),
            entries,
            search.selected.saturating_sub(search.scroll),
        ))
    }

    pub(super) fn search_match_count(&self) -> usize {
        self.search
            .as_ref()
            .map_or(0, |_| self.search_matches().len())
    }

    pub(super) fn handle_search_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let UiInput::Key(key) = input else {
            return match input {
                UiInput::Pointer(pointer) => self.handle_pointer(*pointer, ids, clock),
                UiInput::Paste(value) => self.update_search_query(|query| query.paste(value)),
                UiInput::PasteAnnotated(payload) => {
                    self.update_search_query(|query| query.paste(&payload.content))
                }
                UiInput::Resize { .. } | UiInput::HostFocusGained | UiInput::Key(_) => Vec::new(),
            };
        };
        match *key {
            UiKey::Escape => self.close_overlay(),
            UiKey::Enter => return self.execute_search_selected(),
            UiKey::Backspace => {
                if let Some(search) = &mut self.search {
                    search.query.backspace();
                    search.selected = 0;
                    search.scroll = 0;
                }
            }
            UiKey::Move {
                movement: crate::ports::editor::CursorMovement::VisualUp,
                ..
            } => self.move_search(-1),
            UiKey::Move {
                movement: crate::ports::editor::CursorMovement::VisualDown,
                ..
            } => self.move_search(1),
            UiKey::Move { movement, .. } => {
                if let Some(search) = &mut self.search {
                    search.query.move_cursor(movement);
                }
            }
            UiKey::Delete => {
                if let Some(search) = &mut self.search {
                    search.query.delete();
                }
            }
            UiKey::Character(character) if !character.is_control() => {
                if let Some(search) = &mut self.search {
                    search.query.insert_char(character);
                    search.selected = 0;
                    search.scroll = 0;
                }
            }
            _ => {}
        }
        Vec::new()
    }

    pub(super) fn execute_search_visible_index(&mut self, index: usize) -> Vec<Effect> {
        let absolute = self
            .search
            .as_ref()
            .map_or(index, |search| search.scroll.saturating_add(index));
        self.execute_search_index(absolute)
    }

    fn execute_search_selected(&mut self) -> Vec<Effect> {
        let selected = self.search.as_ref().map_or(0, |search| search.selected);
        self.execute_search_index(selected)
    }

    fn execute_search_index(&mut self, index: usize) -> Vec<Effect> {
        let thought_id = self.search_matches().get(index).copied();
        self.search = None;
        let Some(thought_id) = thought_id else {
            return Vec::new();
        };
        self.clear_range_for_focus_change();
        self.board_viewport = self.board_viewport.follow_focus();
        self.scroll_geometry = None;
        self.reduce(Action::FocusThought(Some(thought_id)))
    }

    fn move_search(&mut self, delta: isize) {
        let count = self.search_matches().len();
        let visible = self
            .layout
            .as_ref()
            .and_then(|layout| layout.overlay.as_ref())
            .map_or(1, |overlay| overlay.items.len().max(1));
        let Some(search) = &mut self.search else {
            return;
        };
        search.selected = search
            .selected
            .saturating_add_signed(delta)
            .min(count.saturating_sub(1));
        if search.selected < search.scroll {
            search.scroll = search.selected;
        } else if search.selected >= search.scroll.saturating_add(visible) {
            search.scroll = search.selected + 1 - visible;
        }
    }

    fn search_matches(&self) -> Vec<ThoughtId> {
        let Some(search) = &self.search else {
            return Vec::new();
        };
        let terms = search
            .query
            .text()
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        self.state
            .board
            .live_thoughts()
            .into_iter()
            .filter(|thought| {
                let content = thought.content.to_lowercase();
                terms.iter().all(|term| content.contains(term))
            })
            .map(|thought| thought.id)
            .collect()
    }

    fn update_search_query(&mut self, update: impl FnOnce(&mut QueryEditor)) -> Vec<Effect> {
        if let Some(search) = &mut self.search {
            update(&mut search.query);
            search.selected = 0;
            search.scroll = 0;
        }
        Vec::new()
    }
}

fn excerpt(content: &str) -> String {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return "(empty thought)".to_owned();
    }
    let mut graphemes = normalized.graphemes(true);
    let visible = graphemes.by_ref().take(52).collect::<String>();
    if graphemes.next().is_some() {
        format!("{visible}…")
    } else {
        visible
    }
}
