//! Shared policy for exchanging selected runs with adjacent Board items.

use crate::domain::BoardItemId;

use super::{ApplicationError, ApplicationResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MoveStep {
    pub(crate) item_id: BoardItemId,
    pub(crate) from: usize,
    pub(crate) to: usize,
}

pub(crate) fn selected_move_steps(
    order: &[BoardItemId],
    selected: &[BoardItemId],
    delta: isize,
) -> ApplicationResult<Vec<MoveStep>> {
    if selected.len() < 2 || !matches!(delta, -1 | 1) {
        return Err(ApplicationError::InvalidState);
    }
    if order
        .iter()
        .copied()
        .filter(|id| selected.contains(id))
        .collect::<Vec<_>>()
        != selected
    {
        return Err(ApplicationError::InvalidState);
    }
    let mut runs = Vec::new();
    let mut cursor = 0;
    while cursor < order.len() {
        if !selected.contains(&order[cursor]) {
            cursor += 1;
            continue;
        }
        let start = cursor;
        while cursor < order.len() && selected.contains(&order[cursor]) {
            cursor += 1;
        }
        runs.push((start, cursor - 1));
    }
    let mut steps = Vec::new();
    if delta < 0 {
        for (start, end) in runs {
            if start > 0 {
                steps.push(MoveStep {
                    item_id: order[start - 1],
                    from: start - 1,
                    to: end,
                });
            }
        }
    } else {
        for (start, end) in runs.into_iter().rev() {
            if end + 1 < order.len() {
                steps.push(MoveStep {
                    item_id: order[end + 1],
                    from: end + 1,
                    to: start,
                });
            }
        }
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use crate::{
        adapters::memory::FakeIdGenerator, domain::BoardItemId, ports::environment::IdGenerator,
    };

    use super::selected_move_steps;

    #[test]
    fn disjoint_runs_exchange_only_adjacent_unselected_neighbors() {
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let order = [
            BoardItemId::Thought(ids.thought_id()),
            BoardItemId::Separator(ids.separator_id()),
            BoardItemId::Thought(ids.thought_id()),
            BoardItemId::Thought(ids.thought_id()),
            BoardItemId::Separator(ids.separator_id()),
        ];
        let selected = [order[1], order[3]];
        let up = selected_move_steps(&order, &selected, -1).expect("up");
        assert_eq!(
            up.iter()
                .map(|step| (step.item_id, step.from, step.to))
                .collect::<Vec<_>>(),
            [(order[0], 0, 1), (order[2], 2, 3)]
        );
        let down = selected_move_steps(&order, &selected, 1).expect("down");
        assert_eq!(
            down.iter()
                .map(|step| (step.item_id, step.from, step.to))
                .collect::<Vec<_>>(),
            [(order[4], 4, 3), (order[2], 2, 1)]
        );
    }

    #[test]
    fn edge_runs_stay_put_and_whole_board_selection_has_no_history_steps() {
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let order = [
            BoardItemId::Thought(ids.thought_id()),
            BoardItemId::Thought(ids.thought_id()),
            BoardItemId::Thought(ids.thought_id()),
        ];
        assert!(
            selected_move_steps(&order, &[order[0], order[1]], -1)
                .expect("up edge")
                .is_empty()
        );
        assert!(
            selected_move_steps(&order, &[order[1], order[2]], 1)
                .expect("down edge")
                .is_empty()
        );
        assert!(
            selected_move_steps(&order, &order, -1)
                .expect("whole board")
                .is_empty()
        );
        assert!(
            selected_move_steps(&order, &order, 1)
                .expect("whole board")
                .is_empty()
        );
    }
}
