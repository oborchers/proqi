//! Responsive header, footer, and labeled action geometry.

use ratatui_core::layout::Rect;

use crate::{application::InteractionMode, ui::KeyBindings};

use super::HitTarget;

pub(super) struct ChromeLayout {
    pub(super) header: Rect,
    pub(super) board: Rect,
    pub(super) footer: Rect,
    pub(super) status: Rect,
    pub(super) name: Rect,
    pub(super) state: Rect,
    pub(super) actions: Rect,
    pub(super) agents: Rect,
}

pub(super) fn compute(area: Rect, has_agents: bool, has_status: bool) -> ChromeLayout {
    let header_height = 0;
    let available = area.height.saturating_sub(header_height);
    let actions_height = u16::from(available >= 2);
    let state_height = u16::from(available >= 3);
    let name_height = u16::from(available >= 4);
    let agents_height = u16::from(has_agents && available >= 5);
    let status_height = u16::from(has_status && available >= 6);
    let chrome_height = actions_height + state_height + name_height + agents_height + status_height;
    let gap_height = u16::from(available.saturating_sub(chrome_height) >= 4);
    let footer_height = chrome_height + gap_height;
    let header = Rect::new(area.x, area.y, area.width, header_height);
    let board = Rect::new(
        area.x,
        area.y.saturating_add(header_height),
        area.width,
        available.saturating_sub(footer_height),
    );
    let footer = Rect::new(area.x, board.bottom(), area.width, footer_height);
    let mut row = footer.y.saturating_add(gap_height);
    let status = Rect::new(area.x, row, area.width, status_height);
    row = row.saturating_add(status_height);
    let name = Rect::new(area.x, row, area.width, name_height);
    row = row.saturating_add(name_height);
    let state = Rect::new(area.x, row, area.width, state_height);
    row = row.saturating_add(state_height);
    let actions = Rect::new(area.x, row, area.width, actions_height);
    row = row.saturating_add(actions_height);
    let agents = Rect::new(area.x, row, area.width, agents_height);
    ChromeLayout {
        header,
        board,
        footer,
        status,
        name,
        state,
        actions,
        agents,
    }
}

pub(super) fn controls(
    area: Rect,
    mode: InteractionMode,
    persistence_failed: bool,
    retry_available: bool,
    has_focus: bool,
    keys: &KeyBindings,
) -> Vec<(HitTarget, Rect)> {
    if area.height == 0 || area.width == 0 {
        return Vec::new();
    }
    let candidates = control_candidates(
        area.width,
        mode,
        persistence_failed,
        retry_available,
        has_focus,
        keys,
    );
    let inset_width = area.width.saturating_sub(4);
    place(
        Rect::new(area.x.saturating_add(2), area.y, inset_width, area.height),
        &candidates,
    )
}

fn control_candidates(
    width: u16,
    mode: InteractionMode,
    persistence_failed: bool,
    retry_available: bool,
    has_focus: bool,
    keys: &KeyBindings,
) -> Vec<(HitTarget, u16)> {
    let editor_mode = matches!(
        mode,
        InteractionMode::Compose | InteractionMode::Edit { .. }
    );
    if let Some(failure) = failure_candidates(persistence_failed, retry_available, mode, keys) {
        return failure;
    }
    if let Some(unfocused) = unfocused_candidates(width, mode, has_focus, keys) {
        return unfocused;
    }
    if width < 24 && editor_mode {
        candidates(
            &[(HitTarget::ExitEdit, false), (HitTarget::Help, true)],
            mode,
            keys,
        )
    } else if width < 24 {
        candidates(
            &[(HitTarget::Commands, true), (HitTarget::Help, true)],
            mode,
            keys,
        )
    } else if (width < 60 && editor_mode) || matches!(mode, InteractionMode::Compose) {
        candidates(
            &[
                (HitTarget::ExitEdit, false),
                (HitTarget::Commands, false),
                (HitTarget::Help, false),
            ],
            mode,
            keys,
        )
    } else if width < 60 {
        candidates(
            &[
                (HitTarget::Insert, false),
                (HitTarget::Commands, false),
                (HitTarget::Help, false),
            ],
            mode,
            keys,
        )
    } else if editor_mode {
        candidates(
            &[
                (HitTarget::ExitEdit, false),
                (HitTarget::Copy, false),
                (HitTarget::Cut, false),
                (HitTarget::Undo, false),
                (HitTarget::Commands, false),
                (HitTarget::Help, false),
            ],
            mode,
            keys,
        )
    } else {
        candidates(
            &[
                (HitTarget::Insert, false),
                (HitTarget::Copy, false),
                (HitTarget::Cut, false),
                (HitTarget::Delete, false),
                (HitTarget::Select, false),
                (HitTarget::Undo, false),
                (HitTarget::Search, false),
                (HitTarget::Commands, false),
                (HitTarget::Help, false),
            ],
            mode,
            keys,
        )
    }
}

fn failure_candidates(
    failed: bool,
    retry: bool,
    mode: InteractionMode,
    keys: &KeyBindings,
) -> Option<Vec<(HitTarget, u16)>> {
    let items = if failed && retry {
        &[
            (HitTarget::Retry, false),
            (HitTarget::ExportRecovery, false),
            (HitTarget::Help, false),
        ][..]
    } else if failed {
        &[(HitTarget::ExportRecovery, false), (HitTarget::Help, false)][..]
    } else {
        return None;
    };
    Some(candidates(items, mode, keys))
}

fn unfocused_candidates(
    width: u16,
    mode: InteractionMode,
    has_focus: bool,
    keys: &KeyBindings,
) -> Option<Vec<(HitTarget, u16)>> {
    if has_focus || !matches!(mode, InteractionMode::Board) {
        return None;
    }
    let items = if width < 24 {
        &[(HitTarget::Insert, false), (HitTarget::Help, true)][..]
    } else {
        &[
            (HitTarget::Insert, false),
            (HitTarget::Commands, false),
            (HitTarget::Help, false),
        ][..]
    };
    Some(candidates(items, mode, keys))
}

fn candidates(
    items: &[(HitTarget, bool)],
    mode: InteractionMode,
    keys: &KeyBindings,
) -> Vec<(HitTarget, u16)> {
    items
        .iter()
        .filter_map(|&(target, compact)| {
            crate::ui::control_labels::action_width(target, compact, mode, keys)
                .map(|width| (target, width))
        })
        .collect()
}

fn place(area: Rect, candidates: &[(HitTarget, u16)]) -> Vec<(HitTarget, Rect)> {
    let mut controls = Vec::new();
    let mut x = area.x;
    for &(target, width) in candidates {
        let gap = u16::from(!controls.is_empty());
        if x.saturating_add(gap).saturating_add(width) > area.right() {
            break;
        }
        x = x.saturating_add(gap);
        controls.push((target, Rect::new(x, area.y, width, 1)));
        x = x.saturating_add(width);
    }
    controls
}
