//! One named work policy for bounded invocation filesystem discovery.

use std::{collections::BTreeSet, sync::Arc};

use crate::ports::invocation::{
    InvocationCancellation, InvocationCompleteness, InvocationDiscoveryStage,
    InvocationIncompleteReason,
};

#[derive(Clone, Copy)]
pub(super) struct InvocationWorkBudgetPolicy {
    pub(super) roots: usize,
    pub(super) entries: usize,
    pub(super) visited_paths: usize,
    pub(super) recursive_depth: usize,
}

pub(super) const WORK_BUDGET: InvocationWorkBudgetPolicy = InvocationWorkBudgetPolicy {
    roots: 128,
    entries: 2_048,
    visited_paths: 8_192,
    recursive_depth: 6,
};

pub(in crate::adapters::invocation) struct WorkBudget {
    roots: usize,
    entries: usize,
    paths: usize,
    exhausted: BTreeSet<WorkLimit>,
    cancellation: Arc<dyn InvocationCancellation>,
    completeness: InvocationCompleteness,
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum WorkLimit {
    Root,
    Entry,
    Path,
    Cancelled,
}

impl WorkBudget {
    pub(in crate::adapters::invocation) fn new(
        cancellation: Arc<dyn InvocationCancellation>,
    ) -> Self {
        Self {
            roots: 0,
            entries: 0,
            paths: 0,
            exhausted: BTreeSet::new(),
            cancellation,
            completeness: InvocationCompleteness::Complete,
        }
    }

    pub(in crate::adapters::invocation) fn admit_initial_roots(&mut self, observed: usize) {
        self.roots = observed.min(WORK_BUDGET.roots);
        if observed > WORK_BUDGET.roots {
            self.exhausted.insert(WorkLimit::Root);
            self.completeness
                .add(InvocationIncompleteReason::RootBudget {
                    observed,
                    limit: WORK_BUDGET.roots,
                });
        }
    }

    pub(in crate::adapters::invocation) fn admit_root(&mut self) -> bool {
        if self.roots >= WORK_BUDGET.roots {
            self.exhausted.insert(WorkLimit::Root);
            self.completeness
                .add(InvocationIncompleteReason::RootBudget {
                    observed: WORK_BUDGET.roots.saturating_add(1),
                    limit: WORK_BUDGET.roots,
                });
            return false;
        }
        self.roots = self.roots.saturating_add(1);
        true
    }

    pub(in crate::adapters::invocation) fn root_exhausted(&self) -> bool {
        self.exhausted.contains(&WorkLimit::Root)
    }

    pub(in crate::adapters::invocation) fn remaining_paths(&self) -> usize {
        WORK_BUDGET.visited_paths.saturating_sub(self.paths)
    }

    pub(in crate::adapters::invocation) fn note_path_overflow(&mut self) {
        self.exhausted.insert(WorkLimit::Path);
        self.completeness
            .add(InvocationIncompleteReason::PathBudget {
                observed: WORK_BUDGET.visited_paths.saturating_add(1),
                limit: WORK_BUDGET.visited_paths,
            });
    }

    pub(in crate::adapters::invocation) fn visit_path(&mut self) -> bool {
        if self.paths >= WORK_BUDGET.visited_paths {
            self.note_path_overflow();
            return false;
        }
        self.paths = self.paths.saturating_add(1);
        true
    }

    pub(in crate::adapters::invocation) fn admit_entry(&mut self) -> bool {
        if self.entries >= WORK_BUDGET.entries {
            self.exhausted.insert(WorkLimit::Entry);
            self.completeness
                .add(InvocationIncompleteReason::EntryBudget {
                    observed: WORK_BUDGET.entries.saturating_add(1),
                    limit: WORK_BUDGET.entries,
                });
            return false;
        }
        self.entries = self.entries.saturating_add(1);
        true
    }

    pub(in crate::adapters::invocation) fn note_depth(&mut self, observed: usize) {
        self.completeness
            .add(InvocationIncompleteReason::RecursiveDepth {
                observed,
                limit: WORK_BUDGET.recursive_depth,
            });
    }

    pub(in crate::adapters::invocation) fn observe_cancellation(&mut self) -> bool {
        if !self.cancellation.is_cancelled() {
            return false;
        }
        if self.exhausted.insert(WorkLimit::Cancelled) {
            self.completeness
                .add(InvocationIncompleteReason::Cancelled {
                    stage: InvocationDiscoveryStage::Filesystem,
                });
        }
        true
    }

    pub(in crate::adapters::invocation) fn should_stop(&self) -> bool {
        self.exhausted.contains(&WorkLimit::Entry)
            || self.exhausted.contains(&WorkLimit::Cancelled)
            || (self.exhausted.contains(&WorkLimit::Path)
                && self.paths >= WORK_BUDGET.visited_paths)
    }

    pub(in crate::adapters::invocation) fn finish(
        mut self,
        other: &InvocationCompleteness,
    ) -> InvocationCompleteness {
        self.completeness.merge(other);
        self.completeness
    }
}
