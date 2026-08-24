//! Terminal-independent searchable session browser state and geometry.

mod geometry;
mod management;

use ratatui_core::layout::Rect;
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    domain::{SessionId, Timestamp},
    ports::{runtime::InstanceInfo, store::SessionHit},
};

use super::{PointerButton, PointerInput, PointerKind, UiInput, UiKey};

/// Runtime availability shown beside one durable session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowserAvailability {
    /// Another verified process currently owns the session.
    Active(InstanceInfo),
    /// Session can be leased and resumed normally.
    Resumable,
    /// Stale crash metadata was recovered during this browser scan.
    Recovered,
    /// Session is recoverably deleted and cannot be opened.
    Trashed,
}

impl BrowserAvailability {
    /// Stable user-facing state label.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Active(_) => "active",
            Self::Resumable => "resumable",
            Self::Recovered => "recovered",
            Self::Trashed => "trashed",
        }
    }
}

/// One search result paired with verified runtime availability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionBrowserItem {
    /// Durable search projection.
    pub hit: SessionHit,
    /// Runtime and trash state observed before the browser opened.
    pub availability: BrowserAvailability,
}

/// Relative recency section rendered in the result list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecencyGroup {
    /// Active within the latest 24 hours.
    Today,
    /// Active between 24 and 48 hours ago.
    Yesterday,
    /// Active between two and seven days ago.
    PreviousWeek,
    /// Older than seven days.
    Older,
}

impl RecencyGroup {
    /// Stable heading text.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Yesterday => "Yesterday",
            Self::PreviousWeek => "Previous 7 days",
            Self::Older => "Older",
        }
    }
}

/// Geometry for one visible result and its optional narrow detail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserEntryLayout {
    /// Index into the browser's complete item collection.
    pub item_index: usize,
    /// Optional recency heading immediately above the result.
    pub group: Option<(RecencyGroup, Rect)>,
    /// Clickable result row.
    pub row: Rect,
    /// Inline detail shown for the selected result in a narrow pane.
    pub inline_detail: Option<Rect>,
}

/// Complete browser geometry shared by rendering and mouse handling.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserLayout {
    /// Complete frame.
    pub area: Rect,
    /// Search header.
    pub header: Rect,
    /// Scrollable result region.
    pub results: Rect,
    /// Wide-screen detail pane.
    pub detail: Option<Rect>,
    /// Visible result geometry.
    pub entries: Vec<BrowserEntryLayout>,
    /// Clickable cancellation footer.
    pub footer: Rect,
}

pub(super) enum BrowserHit {
    Item(usize),
    Rename,
    Trash,
    Cancel,
    None,
}

/// Result of handling one browser input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowserAction {
    /// Continue browsing.
    Continue,
    /// Open this typed session after the browser restores the terminal.
    Open(SessionId),
    /// Persist a new optional name and reopen the refreshed browser.
    Rename {
        /// Session to rename.
        session_id: SessionId,
        /// Empty input clears the optional name.
        name: Option<String>,
    },
    /// Move this session into recoverable trash.
    Trash(SessionId),
    /// Leave without opening a session.
    Cancel,
}

/// Searchable, responsive session picker.
pub struct SessionBrowser {
    items: Vec<SessionBrowserItem>,
    filtered: Vec<usize>,
    query: String,
    selected: usize,
    first_visible: usize,
    now: Timestamp,
    layout: Option<BrowserLayout>,
    rename: Option<management::RenameState>,
    /// Visible explanation for blocked or ambiguous actions.
    pub status: Option<String>,
}

impl SessionBrowser {
    /// Construct a browser in current ranking order.
    #[must_use]
    pub fn new(items: Vec<SessionBrowserItem>, now: Timestamp) -> Self {
        let filtered = (0..items.len()).collect();
        Self {
            items,
            filtered,
            query: String::new(),
            selected: 0,
            first_visible: 0,
            now,
            layout: None,
            rename: None,
            status: None,
        }
    }

    /// Current case-insensitive search text.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Active rename input, when the browser is editing a session name.
    #[must_use]
    pub fn rename_value(&self) -> Option<&str> {
        self.rename.as_ref().map(|rename| rename.value.as_str())
    }

    /// Search results in their storage-defined ranking order.
    pub fn visible_items(&self) -> impl Iterator<Item = (usize, &SessionBrowserItem)> {
        self.filtered
            .iter()
            .copied()
            .map(|index| (index, &self.items[index]))
    }

    /// Currently selected item, when the search has results.
    #[must_use]
    pub fn selected_item(&self) -> Option<(usize, &SessionBrowserItem)> {
        self.filtered
            .get(self.selected)
            .copied()
            .map(|index| (index, &self.items[index]))
    }

    /// Relative recency group for one item.
    #[must_use]
    pub fn group_for(&self, item: &SessionBrowserItem) -> RecencyGroup {
        let age = self
            .now
            .as_millis()
            .saturating_sub(item.hit.last_active_at.as_millis());
        if age <= 86_400_000 {
            RecencyGroup::Today
        } else if age <= 172_800_000 {
            RecencyGroup::Yesterday
        } else if age <= 604_800_000 {
            RecencyGroup::PreviousWeek
        } else {
            RecencyGroup::Older
        }
    }

    /// Compact last-activity label for result rows.
    #[must_use]
    pub fn activity_label(&self, item: &SessionBrowserItem) -> String {
        let age = self
            .now
            .as_millis()
            .saturating_sub(item.hit.last_active_at.as_millis())
            .max(0);
        if age < 60_000 {
            "now".to_owned()
        } else if age < 3_600_000 {
            format!("{}m ago", age / 60_000)
        } else if age < 86_400_000 {
            format!("{}h ago", age / 3_600_000)
        } else {
            format!("{}d ago", age / 86_400_000)
        }
    }

    /// Recompute authoritative frame and hit-test geometry.
    pub fn prepare_frame(&mut self, area: Rect) -> BrowserLayout {
        if self.selected < self.first_visible {
            self.first_visible = self.selected;
        }
        let mut layout = self.compute_layout(area);
        let selected_index = self.filtered.get(self.selected).copied();
        if selected_index
            .is_some_and(|index| !layout.entries.iter().any(|entry| entry.item_index == index))
        {
            self.first_visible = self.selected;
            layout = self.compute_layout(area);
        }
        self.layout = Some(layout.clone());
        layout
    }

    /// Apply one normalized terminal event.
    pub fn handle(&mut self, input: UiInput) -> BrowserAction {
        self.status = None;
        if self.rename.is_some() {
            return self.handle_rename(input);
        }
        match input {
            UiInput::Key(UiKey::Quit | UiKey::Escape) => BrowserAction::Cancel,
            UiInput::Key(UiKey::Enter) => self.activate(),
            UiInput::Key(UiKey::Backspace | UiKey::Delete) => {
                if let Some((index, _)) = self.query.grapheme_indices(true).next_back() {
                    self.query.truncate(index);
                }
                self.refilter();
                BrowserAction::Continue
            }
            UiInput::Key(UiKey::Move { movement, .. }) => {
                use crate::ports::editor::CursorMovement;
                match movement {
                    CursorMovement::VisualUp
                    | CursorMovement::GraphemeBack
                    | CursorMovement::WordBack
                    | CursorMovement::LineStart
                    | CursorMovement::DocumentStart => self.move_selection(-1),
                    _ => self.move_selection(1),
                }
                BrowserAction::Continue
            }
            UiInput::Key(UiKey::Character(character)) => match character {
                'R' if self.query.is_empty() => self.begin_rename(),
                'D' if self.query.is_empty() => self.trash_selected(),
                _ => {
                    self.handle_character(character);
                    BrowserAction::Continue
                }
            },
            UiInput::Paste(text) => {
                self.query.push_str(&text.replace(['\r', '\n'], " "));
                self.refilter();
                BrowserAction::Continue
            }
            UiInput::PasteAnnotated(payload) => {
                self.query
                    .push_str(&payload.content.replace(['\r', '\n'], " "));
                self.refilter();
                BrowserAction::Continue
            }
            UiInput::Pointer(pointer) => self.handle_pointer(pointer),
            UiInput::Resize { .. } | UiInput::HostFocusGained | UiInput::Key(_) => {
                BrowserAction::Continue
            }
        }
    }

    fn refilter(&mut self) {
        let query = self.query.to_lowercase();
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let searchable = searchable_text(item).to_lowercase();
                query
                    .split_whitespace()
                    .all(|word| searchable.contains(word))
                    .then_some(index)
            })
            .collect();
        self.selected = 0;
        self.first_visible = 0;
        self.layout = None;
    }

    fn handle_character(&mut self, character: char) {
        match (self.query.is_empty(), character) {
            (true, 'j') => self.move_selection(1),
            (true, 'k') => self.move_selection(-1),
            _ => {
                self.query.push(character);
                self.refilter();
            }
        }
    }

    fn move_selection(&mut self, amount: isize) {
        let last = self.filtered.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(amount).min(last);
        self.layout = None;
    }

    fn handle_pointer(&mut self, pointer: PointerInput) -> BrowserAction {
        if matches!(
            pointer.kind,
            PointerKind::ScrollUp | PointerKind::ScrollDown
        ) {
            self.move_selection(if matches!(pointer.kind, PointerKind::ScrollUp) {
                -1
            } else {
                1
            });
            return BrowserAction::Continue;
        }
        if !matches!(pointer.kind, PointerKind::Down(PointerButton::Left)) {
            return BrowserAction::Continue;
        }
        let Some(layout) = &self.layout else {
            return BrowserAction::Continue;
        };
        match layout.hit_test(pointer.column, pointer.row) {
            BrowserHit::Cancel => BrowserAction::Cancel,
            BrowserHit::Rename => self.begin_rename(),
            BrowserHit::Trash => self.trash_selected(),
            BrowserHit::Item(item_index) => {
                let Some(position) = self.filtered.iter().position(|index| *index == item_index)
                else {
                    return BrowserAction::Continue;
                };
                self.selected = position;
                self.activate()
            }
            BrowserHit::None => BrowserAction::Continue,
        }
    }

    fn activate(&mut self) -> BrowserAction {
        let Some((_, item)) = self.selected_item() else {
            self.status = Some("No matching session".to_owned());
            return BrowserAction::Continue;
        };
        match &item.availability {
            BrowserAvailability::Resumable | BrowserAvailability::Recovered => {
                BrowserAction::Open(item.hit.id)
            }
            BrowserAvailability::Active(instance) => {
                self.status = Some(format!("Session is active in process {}", instance.pid));
                BrowserAction::Continue
            }
            BrowserAvailability::Trashed => {
                self.status = Some("Restore this session before opening it".to_owned());
                BrowserAction::Continue
            }
        }
    }
}

fn searchable_text(item: &SessionBrowserItem) -> String {
    let mut values = vec![
        item.hit.id.to_string(),
        item.hit.name.clone().unwrap_or_default(),
        item.hit.origin_cwd.to_string_lossy().into_owned(),
        item.hit.last_opened_cwd.to_string_lossy().into_owned(),
        item.hit.excerpt.clone(),
        item.hit.search_content.clone(),
    ];
    values.extend(item.hit.previews.iter().cloned());
    if let Some(context) = &item.hit.integration_context {
        values.extend([
            context.provider.clone(),
            context.agent_kind.clone(),
            context.agent_name.clone(),
            context.workspace_hint.clone().unwrap_or_default(),
            context.tab_hint.clone().unwrap_or_default(),
            context.pane_hint.clone().unwrap_or_default(),
        ]);
    }
    values.join("\n")
}
