//! Content-sized footer controls and hit geometry.

use ratatui_core::layout::Rect;
use unicode_width::UnicodeWidthStr as _;

use crate::{
    domain::Direction,
    ports::agent::{AgentTarget, SubmissionDisposition},
};

use super::{HitTarget, LayoutSnapshot, OverlayLayout};

pub(super) fn overlay_layout(
    area: Rect,
    item_count: usize,
    preferred_rows: usize,
    cover_width: bool,
) -> OverlayLayout {
    let requested_height = overlay_height(preferred_rows);
    let height = area.height.clamp(1, requested_height.max(5));
    let width = if cover_width || height == area.height {
        area.width
    } else {
        area.width.clamp(1, 58)
    };
    let modal = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let items = (0..item_count.min(usize::from(height.saturating_sub(3))))
        .map(|index| {
            Rect::new(
                modal.x.saturating_add(1),
                modal
                    .y
                    .saturating_add(2)
                    .saturating_add(u16::try_from(index).unwrap_or(u16::MAX)),
                modal.width.saturating_sub(2),
                1,
            )
        })
        .collect();
    OverlayLayout {
        area: modal,
        items,
        close: Rect::new(modal.right().saturating_sub(3), modal.y, 3, 1),
    }
}

pub(super) fn overlay_height(preferred_rows: usize) -> u16 {
    u16::try_from(preferred_rows.saturating_add(3)).unwrap_or(u16::MAX)
}

pub(super) fn configure_footer_summary(
    layout: &mut LayoutSnapshot,
    summary: String,
    session_name: String,
) {
    let area = inset(layout.footer_name);
    if area.height > 0 {
        layout.controls.push((
            HitTarget::RenameSession,
            Rect::new(area.x, area.y, area.width, 1),
        ));
    }
    layout.footer_summary = summary;
    layout.footer_session_name = session_name;
}

pub(super) fn configure_agent_controls(
    layout: &mut LayoutSnapshot,
    targets: &[AgentTarget],
    selection: Option<SubmissionDisposition>,
) {
    let area = inset(layout.footer_agents);
    if area.height == 0 {
        return;
    }
    let mut x = area.x;
    if let Some(disposition) = selection {
        for target in targets.iter().filter(|target| target.delivery.supports()) {
            let label_width = agent_width(target);
            push(
                layout,
                &mut x,
                area,
                HitTarget::Deliver(target.direction, disposition),
                label_width,
            );
        }
        return;
    }
    for target in targets {
        let label_width = agent_width(target);
        push(
            layout,
            &mut x,
            area,
            HitTarget::Agent(target.direction),
            label_width,
        );
    }
    for disposition in [
        SubmissionDisposition::RemoveAfterSuccess,
        SubmissionDisposition::Keep,
    ] {
        let eligible = targets
            .iter()
            .filter(|target| target.delivery.supports())
            .collect::<Vec<_>>();
        let target = match eligible.as_slice() {
            [] => continue,
            [only] => HitTarget::Deliver(only.direction, disposition),
            _ => HitTarget::BeginDelivery(disposition),
        };
        let label_width = match disposition {
            SubmissionDisposition::RemoveAfterSuccess => 9,
            SubmissionDisposition::Keep => 16,
        };
        push(layout, &mut x, area, target, label_width);
    }
}

fn push(layout: &mut LayoutSnapshot, x: &mut u16, area: Rect, target: HitTarget, width: u16) {
    let gap = if *x > area.x { 3 } else { 0 };
    let start = x.saturating_add(gap);
    if start.saturating_add(width) <= area.right() {
        layout
            .controls
            .push((target, Rect::new(start, area.y, width, 1)));
        *x = start.saturating_add(width);
    }
}

fn agent_width(target: &AgentTarget) -> u16 {
    let direction = match target.direction {
        Direction::Up | Direction::Right | Direction::Down | Direction::Left => 1,
    };
    direction + 1 + width(target.agent_kind.as_str())
}

fn width(value: &str) -> u16 {
    u16::try_from(value.width()).unwrap_or(u16::MAX)
}

fn inset(area: Rect) -> Rect {
    Rect::new(
        area.x.saturating_add(2),
        area.y,
        area.width.saturating_sub(4),
        area.height,
    )
}
