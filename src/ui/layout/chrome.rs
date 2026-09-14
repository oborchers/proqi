//! Responsive header, footer, and labeled action geometry.

use ratatui_core::layout::Rect;

use crate::ui::{ShortcutContext, ShortcutRegistry};

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
    context: ShortcutContext,
    retry_available: bool,
    has_focus: bool,
    history_available: (bool, bool),
    keys: &ShortcutRegistry,
) -> Vec<(HitTarget, Rect)> {
    if area.height == 0 || area.width == 0 {
        return Vec::new();
    }
    let candidates = control_candidates(
        area.width,
        context,
        retry_available,
        has_focus,
        history_available,
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
    context: ShortcutContext,
    retry_available: bool,
    has_focus: bool,
    history_available: (bool, bool),
    keys: &ShortcutRegistry,
) -> Vec<(HitTarget, u16)> {
    let compose_mode = context == ShortcutContext::Compose;
    let edit_mode = context == ShortcutContext::Edit;
    if let Some(failure) =
        failure_candidates(context == ShortcutContext::Recovery, retry_available, keys)
    {
        return failure;
    }
    if let Some(unfocused) = unfocused_candidates(width, context, has_focus, keys) {
        return unfocused;
    }
    if compose_mode {
        history_candidates(
            &[
                (HitTarget::ExitEdit, width < 16),
                (HitTarget::Undo, false),
                (HitTarget::Redo, false),
            ],
            context,
            history_available,
            keys,
        )
    } else if width < 60 && edit_mode {
        candidates(&[(HitTarget::ExitEdit, width < 16)], context, keys)
    } else if width < 24 {
        candidates(
            &[(HitTarget::Commands, true), (HitTarget::Help, true)],
            context,
            keys,
        )
    } else if width < 60 {
        candidates(
            &[
                (HitTarget::Insert, false),
                (HitTarget::Commands, false),
                (HitTarget::Help, false),
            ],
            context,
            keys,
        )
    } else if edit_mode {
        history_candidates(
            &[
                (HitTarget::ExitEdit, false),
                (HitTarget::Copy, false),
                (HitTarget::Cut, false),
                (HitTarget::Undo, false),
                (HitTarget::Redo, false),
            ],
            context,
            history_available,
            keys,
        )
    } else {
        board_action_candidates(width.saturating_sub(4), context, history_available, keys)
    }
}

fn board_action_candidates(
    available_width: u16,
    context: ShortcutContext,
    history_available: (bool, bool),
    keys: &ShortcutRegistry,
) -> Vec<(HitTarget, u16)> {
    let items = |compact| {
        history_candidates(
            &[
                (HitTarget::Insert, false),
                (HitTarget::Copy, compact),
                (HitTarget::Cut, compact),
                (HitTarget::Delete, false),
                (HitTarget::Select, false),
                (HitTarget::Undo, compact),
                (HitTarget::Redo, compact),
                (HitTarget::Search, false),
                (HitTarget::Commands, false),
                (HitTarget::Help, false),
            ],
            context,
            history_available,
            keys,
        )
    };
    let full = items(false);
    if placed_width(&full) <= available_width {
        full
    } else {
        items(true)
    }
}

fn history_candidates(
    items: &[(HitTarget, bool)],
    context: ShortcutContext,
    history_available: (bool, bool),
    keys: &ShortcutRegistry,
) -> Vec<(HitTarget, u16)> {
    let mut result = candidates(items, context, keys);
    result.retain(|(target, _)| match target {
        HitTarget::Undo => history_available.0,
        HitTarget::Redo => history_available.1,
        _ => true,
    });
    result
}

fn placed_width(candidates: &[(HitTarget, u16)]) -> u16 {
    candidates
        .iter()
        .map(|(_, width)| *width)
        .fold(0_u16, u16::saturating_add)
        .saturating_add(u16::try_from(candidates.len().saturating_sub(1)).unwrap_or(u16::MAX))
}

fn failure_candidates(
    failed: bool,
    retry: bool,
    keys: &ShortcutRegistry,
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
    Some(candidates(
        items,
        crate::ui::ShortcutContext::Recovery,
        keys,
    ))
}

fn unfocused_candidates(
    width: u16,
    context: ShortcutContext,
    has_focus: bool,
    keys: &ShortcutRegistry,
) -> Option<Vec<(HitTarget, u16)>> {
    if has_focus
        || !matches!(
            context,
            ShortcutContext::Board | ShortcutContext::InsertionBoundary
        )
    {
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
    Some(candidates(items, context, keys))
}

fn candidates(
    items: &[(HitTarget, bool)],
    context: crate::ui::ShortcutContext,
    keys: &ShortcutRegistry,
) -> Vec<(HitTarget, u16)> {
    items
        .iter()
        .filter_map(|&(target, compact)| {
            crate::ui::control_labels::action_width(target, compact, context, keys)
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
