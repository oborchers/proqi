//! Deterministic per-tick worker-lane fairness.

/// Maximum results one lane may apply before terminal input gets another turn.
pub(super) const RESULTS_PER_LANE_TICK: usize = 8;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct DrainOutcome {
    pub(super) changed: bool,
    pub(super) budget_exhausted: bool,
}

pub(super) fn drain_bounded<T, E>(
    mut receive: impl FnMut() -> Result<Option<T>, E>,
    mut handle: impl FnMut(T) -> Result<bool, E>,
) -> Result<DrainOutcome, E> {
    let mut outcome = DrainOutcome::default();
    for _ in 0..RESULTS_PER_LANE_TICK {
        let Some(item) = receive()? else {
            return Ok(outcome);
        };
        outcome.changed |= handle(item)?;
    }
    outcome.budget_exhausted = true;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, collections::VecDeque, convert::Infallible};

    use super::{RESULTS_PER_LANE_TICK, drain_bounded};

    #[test]
    fn continuous_lane_yields_after_its_explicit_budget() {
        let mut items = (0..100).collect::<VecDeque<_>>();
        let handled = Cell::new(0);
        let outcome = drain_bounded(
            || Ok::<_, Infallible>(items.pop_front()),
            |_| {
                handled.set(handled.get() + 1);
                Ok(true)
            },
        )
        .expect("infallible drain");

        assert_eq!(handled.get(), RESULTS_PER_LANE_TICK);
        assert_eq!(items.len(), 100 - RESULTS_PER_LANE_TICK);
        assert!(outcome.changed);
        assert!(outcome.budget_exhausted);
    }

    #[test]
    fn finite_lane_reports_no_backlog_after_observing_empty() {
        let mut items = VecDeque::from([1, 2]);
        let outcome = drain_bounded(|| Ok::<_, Infallible>(items.pop_front()), |_| Ok(false))
            .expect("infallible drain");

        assert!(!outcome.changed);
        assert!(!outcome.budget_exhausted);
    }
}
