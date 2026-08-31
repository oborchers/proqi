//! Content-sized footer controls and hit geometry.

use ratatui_core::layout::Rect;
use unicode_width::UnicodeWidthStr as _;

use crate::ports::agent::{AgentTarget, SubmissionDisposition};

use super::{HitTarget, LayoutSnapshot, OverlayLayout};

pub(super) fn overlay_layout(
    area: Rect,
    item_count: usize,
    preferred_rows: usize,
    cover_width: bool,
) -> OverlayLayout {
    let modal = modal_area(area, preferred_rows, cover_width);
    let height = modal.height;
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
        .collect::<Vec<_>>();
    let item_headings = vec![None; items.len()];
    OverlayLayout {
        area: modal,
        items,
        item_headings,
        close: Rect::new(modal.right().saturating_sub(3), modal.y, 3, 1),
    }
}

pub(super) fn grouped_overlay_layout(
    area: Rect,
    item_groups: &[bool],
    preferred_rows: usize,
    cover_width: bool,
) -> OverlayLayout {
    let modal = modal_area(area, preferred_rows, cover_width);
    let mut item_y = modal.y.saturating_add(2);
    let bottom = modal.bottom().saturating_sub(1);
    let mut items = Vec::new();
    let mut item_headings = Vec::new();
    for grouped in item_groups.iter().copied() {
        if item_y >= bottom {
            break;
        }
        let heading = (grouped && item_y.saturating_add(2) <= bottom).then(|| {
            let area = Rect::new(
                modal.x.saturating_add(1),
                item_y,
                modal.width.saturating_sub(2),
                1,
            );
            item_y = item_y.saturating_add(1);
            area
        });
        items.push(Rect::new(
            modal.x.saturating_add(1),
            item_y,
            modal.width.saturating_sub(2),
            1,
        ));
        item_headings.push(heading);
        item_y = item_y.saturating_add(1);
    }
    OverlayLayout {
        area: modal,
        items,
        item_headings,
        close: Rect::new(modal.right().saturating_sub(3), modal.y, 3, 1),
    }
}

fn modal_area(area: Rect, preferred_rows: usize, cover_width: bool) -> Rect {
    let requested_height = overlay_height(preferred_rows);
    let height = area.height.clamp(1, requested_height.max(5));
    let width = if cover_width || height == area.height {
        area.width
    } else {
        area.width.clamp(1, 58)
    };
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

pub(super) fn overlay_height(preferred_rows: usize) -> u16 {
    u16::try_from(preferred_rows.saturating_add(3)).unwrap_or(u16::MAX)
}

pub(super) fn configure_footer_summary(
    layout: &mut LayoutSnapshot,
    summary: String,
    session_name: String,
    session_id: Option<String>,
) {
    let area = crate::ui::geometry::inset_horizontal(layout.footer_name, 2);
    if area.height > 0 {
        configure_session_targets(layout, area, &session_name, session_id.as_deref());
    }
    layout.footer_summary = summary;
    layout.footer_session_name = session_name;
    layout.footer_session_id = session_id.filter(|id| {
        area.height > 0
            && width(&layout.footer_session_name)
                .saturating_add(3)
                .saturating_add(width(id))
                <= area.width
    });
}

fn configure_session_targets(
    layout: &mut LayoutSnapshot,
    area: Rect,
    session_name: &str,
    session_id: Option<&str>,
) {
    let name_width = width(session_name);
    let id_width = session_id.map_or(0, width);
    let id_fits =
        session_id.is_some() && name_width.saturating_add(3).saturating_add(id_width) <= area.width;
    let rename_width = if id_fits { name_width } else { area.width };
    layout.controls.push((
        HitTarget::RenameSession,
        Rect::new(area.x, area.y, rename_width, 1),
    ));
    if id_fits {
        layout.controls.push((
            HitTarget::CopySessionId,
            Rect::new(
                area.x.saturating_add(name_width).saturating_add(3),
                area.y,
                id_width,
                1,
            ),
        ));
    }
}

pub(super) fn configure_agent_controls(
    layout: &mut LayoutSnapshot,
    targets: &[AgentTarget],
    selection: Option<SubmissionDisposition>,
    mode: crate::application::InteractionMode,
    keybindings: &crate::ui::KeyBindings,
) {
    let area = crate::ui::geometry::inset_horizontal(layout.footer_agents, 2);
    if area.height == 0 {
        return;
    }
    let mut x = area.x;
    if let Some(disposition) = selection {
        for target in targets.iter().filter(|target| target.delivery.supports()) {
            let label_width = crate::ui::control_labels::agent(target).width();
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
    if matches!(mode, crate::application::InteractionMode::Compose) {
        return;
    }
    for target in targets {
        let label_width = crate::ui::control_labels::agent(target).width();
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
        let label_width =
            crate::ui::control_labels::submission_width(disposition, mode, keybindings);
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

fn width(value: &str) -> u16 {
    u16::try_from(value.width()).unwrap_or(u16::MAX)
}
