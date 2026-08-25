//! Content-sized footer controls and hit geometry.

use ratatui_core::layout::Rect;
use unicode_width::UnicodeWidthStr as _;

use crate::{
    domain::Direction,
    ports::agent::{AgentTarget, SubmissionDisposition},
};

use super::{HitTarget, LayoutSnapshot};

pub(super) fn configure_footer_summary(
    layout: &mut LayoutSnapshot,
    summary: String,
    session_name_width: u16,
) {
    let area = inset(layout.footer_context);
    let summary_width = width(&summary).min(area.width);
    if session_name_width > 0 && area.height > 0 {
        layout.controls.push((
            HitTarget::RenameSession,
            Rect::new(
                area.right().saturating_sub(summary_width),
                area.y,
                session_name_width.min(summary_width),
                1,
            ),
        ));
    }
    layout.footer_summary = summary;
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
            SubmissionDisposition::RemoveAfterSuccess => 21,
            SubmissionDisposition::Keep => 19,
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
    direction + 1 + width(&target.agent_kind)
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
