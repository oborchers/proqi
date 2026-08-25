//! Responsive header, footer, and labeled action geometry.

use ratatui_core::layout::Rect;

use crate::application::InteractionMode;

use super::HitTarget;

pub(super) struct ChromeLayout {
    pub(super) header: Rect,
    pub(super) board: Rect,
    pub(super) footer: Rect,
    pub(super) context: Rect,
    pub(super) actions: Rect,
    pub(super) agents: Rect,
}

pub(super) fn compute(area: Rect, _mode: InteractionMode, has_agents: bool) -> ChromeLayout {
    let header_height = 0;
    let available = area.height.saturating_sub(header_height);
    let actions_height = u16::from(available >= 2);
    let context_height = u16::from(available >= 4);
    let agents_height = u16::from(has_agents && available >= 5);
    let chrome_height = actions_height + context_height + agents_height;
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
    let context = Rect::new(area.x, row, area.width, context_height);
    row = row.saturating_add(context_height);
    let actions = Rect::new(area.x, row, area.width, actions_height);
    row = row.saturating_add(actions_height);
    let agents = Rect::new(area.x, row, area.width, agents_height);
    ChromeLayout {
        header,
        board,
        footer,
        context,
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
) -> Vec<(HitTarget, Rect)> {
    if area.height == 0 || area.width == 0 {
        return Vec::new();
    }
    let candidates = if persistence_failed && retry_available {
        vec![
            (HitTarget::Retry, 8),
            (HitTarget::ExportRecovery, 10),
            (HitTarget::Help, 12),
        ]
    } else if persistence_failed {
        vec![(HitTarget::ExportRecovery, 10), (HitTarget::Help, 12)]
    } else if !has_focus && matches!(mode, InteractionMode::Board) && area.width < 24 {
        vec![(HitTarget::Insert, 7), (HitTarget::Help, 6)]
    } else if !has_focus && matches!(mode, InteractionMode::Board) {
        vec![
            (HitTarget::Insert, 7),
            (HitTarget::Commands, 11),
            (HitTarget::Help, 12),
        ]
    } else if area.width < 24 {
        vec![(HitTarget::Commands, 6), (HitTarget::Help, 6)]
    } else if area.width < 60 && matches!(mode, InteractionMode::Edit { .. }) {
        vec![
            (HitTarget::ExitEdit, 10),
            (HitTarget::Commands, 11),
            (HitTarget::Help, 12),
        ]
    } else if area.width < 60 {
        vec![
            (HitTarget::Insert, 7),
            (HitTarget::Commands, 11),
            (HitTarget::Help, 12),
        ]
    } else if matches!(mode, InteractionMode::Edit { .. }) {
        vec![
            (HitTarget::ExitEdit, 10),
            (HitTarget::Copy, 7),
            (HitTarget::Cut, 6),
            (HitTarget::Undo, 7),
            (HitTarget::Commands, 11),
            (HitTarget::Help, 12),
        ]
    } else {
        vec![
            (HitTarget::Insert, 7),
            (HitTarget::Copy, 7),
            (HitTarget::Cut, 6),
            (HitTarget::Delete, 9),
            (HitTarget::Undo, 7),
            (HitTarget::Search, 9),
            (HitTarget::Commands, 11),
            (HitTarget::Help, 12),
        ]
    };
    let inset_width = area.width.saturating_sub(4);
    place(
        Rect::new(area.x.saturating_add(2), area.y, inset_width, area.height),
        &candidates,
    )
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
