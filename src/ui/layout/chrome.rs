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
    pub(super) status: Option<Rect>,
}

pub(super) fn compute(area: Rect, _mode: InteractionMode, has_status: bool) -> ChromeLayout {
    let header_height = u16::from(area.height > 0);
    let available = area.height.saturating_sub(header_height);
    let actions_height = u16::from(available >= 2);
    let context_height = u16::from(available >= 4);
    let status_height = u16::from(has_status && available >= 7);
    let footer_height = actions_height + context_height + status_height;
    let header = Rect::new(area.x, area.y, area.width, header_height);
    let board = Rect::new(
        area.x,
        area.y.saturating_add(header_height),
        area.width,
        available.saturating_sub(footer_height),
    );
    let footer = Rect::new(area.x, board.bottom(), area.width, footer_height);
    let mut row = footer.y;
    let status = (status_height > 0).then(|| {
        let result = Rect::new(area.x, row, area.width, 1);
        row = row.saturating_add(1);
        result
    });
    let context = Rect::new(area.x, row, area.width, context_height);
    row = row.saturating_add(context_height);
    let actions = Rect::new(area.x, row, area.width, actions_height);
    ChromeLayout {
        header,
        board,
        footer,
        context,
        actions,
        status,
    }
}

pub(super) fn controls(area: Rect, mode: InteractionMode) -> Vec<(HitTarget, Rect)> {
    if area.height == 0 || area.width == 0 {
        return Vec::new();
    }
    let candidates = if area.width < 24 {
        vec![(HitTarget::Commands, 6), (HitTarget::Help, 6)]
    } else if area.width < 60 && matches!(mode, InteractionMode::Edit { .. }) {
        vec![
            (HitTarget::Copy, 7),
            (HitTarget::Undo, 7),
            (HitTarget::Commands, 11),
            (HitTarget::Help, 12),
        ]
    } else if area.width < 60 {
        vec![
            (HitTarget::Insert, 7),
            (HitTarget::Copy, 7),
            (HitTarget::Commands, 11),
            (HitTarget::Help, 12),
        ]
    } else if matches!(mode, InteractionMode::Edit { .. }) {
        vec![
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
    place(area, &candidates)
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
