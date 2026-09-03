//! Typed completeness shared by every invocation-discovery source.

use crate::ports::agent::AgentFailureCode;

/// Invocation-discovery stage whose bounded work may be incomplete.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum InvocationDiscoveryStage {
    /// Filesystem root and definition traversal.
    Filesystem,
    /// Claude installed-plugin registry parsing.
    ClaudePluginRegistry,
    /// Claude plugin manifest parsing.
    ClaudePluginManifest,
    /// Herdr agent rows.
    HerdrAgents,
    /// Herdr workspace rows.
    HerdrWorkspaces,
    /// Herdr tab rows.
    HerdrTabs,
    /// Herdr provider request and response processing.
    HerdrProvider,
}

impl InvocationDiscoveryStage {
    /// Stable content-free diagnostic spelling.
    #[must_use]
    pub const fn diagnostic_code(self) -> &'static str {
        match self {
            Self::Filesystem => "filesystem",
            Self::ClaudePluginRegistry => "claude_plugin_registry",
            Self::ClaudePluginManifest => "claude_plugin_manifest",
            Self::HerdrAgents => "herdr_agents",
            Self::HerdrWorkspaces => "herdr_workspaces",
            Self::HerdrTabs => "herdr_tabs",
            Self::HerdrProvider => "herdr_provider",
        }
    }
}

/// Closed, content-redacted reason why retained discovery results are partial.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum InvocationIncompleteReason {
    /// More valid entries existed than the shared work policy admits.
    EntryBudget {
        /// Smallest proven entry count.
        observed: usize,
        /// Configured entry limit.
        limit: usize,
    },
    /// More filesystem paths existed than the shared work policy visits.
    PathBudget {
        /// Smallest proven path count.
        observed: usize,
        /// Configured path limit.
        limit: usize,
    },
    /// More roots existed than the shared work policy scans.
    RootBudget {
        /// Exact combined root count.
        observed: usize,
        /// Configured root limit.
        limit: usize,
    },
    /// At least one directory existed beyond the recursive depth.
    RecursiveDepth {
        /// Deepest observed unscanned level.
        observed: usize,
        /// Configured recursive depth.
        limit: usize,
    },
    /// The installed-plugin registry exceeded its complete-input bound.
    RegistrySize {
        /// Observed file size, or the smallest byte count proving overflow.
        observed: u64,
        /// Configured registry byte limit.
        limit: u64,
    },
    /// One or more plugin manifests exceeded their complete-input bound.
    ManifestSize {
        /// Largest observed size, or the smallest byte count proving overflow.
        observed: u64,
        /// Configured manifest byte limit.
        limit: u64,
        /// Number of oversized manifests.
        affected: usize,
    },
    /// A provider snapshot contained more rows than its bounded projection.
    ProviderRowBudget {
        /// Typed provider collection.
        stage: InvocationDiscoveryStage,
        /// Exact row count supplied by the provider.
        observed: usize,
        /// Configured projection limit.
        limit: usize,
    },
    /// A provider request failed with a stable content-free classification.
    ProviderFailure {
        /// Provider boundary that failed.
        stage: InvocationDiscoveryStage,
        /// Stable failure classification.
        code: AgentFailureCode,
    },
    /// Runtime shutdown cancelled in-progress discovery.
    Cancelled {
        /// Boundary observing cancellation.
        stage: InvocationDiscoveryStage,
    },
}

impl InvocationIncompleteReason {
    /// Stable reason spelling used only at diagnostics and presentation boundaries.
    #[must_use]
    pub const fn diagnostic_code(&self) -> &'static str {
        match self {
            Self::EntryBudget { .. } => "entry_budget",
            Self::PathBudget { .. } => "path_budget",
            Self::RootBudget { .. } => "root_budget",
            Self::RecursiveDepth { .. } => "recursive_depth",
            Self::RegistrySize { .. } => "registry_size",
            Self::ManifestSize { .. } => "manifest_size",
            Self::ProviderRowBudget { .. } => "provider_row_budget",
            Self::ProviderFailure { .. } => "provider_failure",
            Self::Cancelled { .. } => "cancelled",
        }
    }

    /// Stable content-free stage spelling.
    #[must_use]
    pub const fn stage_code(&self) -> &'static str {
        match self {
            Self::EntryBudget { .. }
            | Self::PathBudget { .. }
            | Self::RootBudget { .. }
            | Self::RecursiveDepth { .. } => InvocationDiscoveryStage::Filesystem.diagnostic_code(),
            Self::RegistrySize { .. } => {
                InvocationDiscoveryStage::ClaudePluginRegistry.diagnostic_code()
            }
            Self::ManifestSize { .. } => {
                InvocationDiscoveryStage::ClaudePluginManifest.diagnostic_code()
            }
            Self::ProviderRowBudget { stage, .. }
            | Self::ProviderFailure { stage, .. }
            | Self::Cancelled { stage } => stage.diagnostic_code(),
        }
    }
}

/// Truthful completeness of one retained invocation-discovery result.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum InvocationCompleteness {
    /// Every accepted source was processed within its declared bounds.
    #[default]
    Complete,
    /// Retained results accompanied by one or more partial-source reasons.
    Incomplete(Vec<InvocationIncompleteReason>),
}

impl InvocationCompleteness {
    /// Whether every accepted source was processed.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }

    /// Stable ordered reasons, empty for complete discovery.
    #[must_use]
    pub fn reasons(&self) -> &[InvocationIncompleteReason] {
        match self {
            Self::Complete => &[],
            Self::Incomplete(reasons) => reasons,
        }
    }

    /// Add one reason while retaining deterministic duplicate-free order.
    pub fn add(&mut self, reason: InvocationIncompleteReason) {
        let reasons = match self {
            Self::Complete => {
                *self = Self::Incomplete(Vec::new());
                let Self::Incomplete(reasons) = self else {
                    return;
                };
                reasons
            }
            Self::Incomplete(reasons) => reasons,
        };
        if !reasons.contains(&reason) {
            reasons.push(reason);
            reasons.sort();
        }
    }

    /// Merge another source without overwriting either source's reasons.
    pub fn merge(&mut self, other: &Self) {
        for reason in other.reasons() {
            self.add(reason.clone());
        }
    }
}

/// Canonical owner for independently refreshed filesystem and provider completeness.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InvocationCompletenessAggregate {
    filesystem: InvocationCompleteness,
    provider: InvocationCompleteness,
}

impl InvocationCompletenessAggregate {
    /// Replace only filesystem and plugin completeness.
    pub fn set_filesystem(&mut self, completeness: InvocationCompleteness) {
        self.filesystem = completeness;
    }

    /// Replace only ephemeral provider completeness.
    pub fn set_provider(&mut self, completeness: InvocationCompleteness) {
        self.provider = completeness;
    }

    /// Clear ephemeral provider completeness when its picker closes.
    pub fn clear_provider(&mut self) {
        self.provider = InvocationCompleteness::Complete;
    }

    /// Combine active sources through this single aggregation owner.
    #[must_use]
    pub fn combined(&self) -> InvocationCompleteness {
        let mut combined = self.filesystem.clone();
        combined.merge(&self.provider);
        combined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_sources_merge_without_overwriting_or_reordering() {
        let mut aggregate = InvocationCompletenessAggregate::default();
        let mut filesystem = InvocationCompleteness::Complete;
        filesystem.add(InvocationIncompleteReason::RootBudget {
            observed: 129,
            limit: 128,
        });
        let mut provider = InvocationCompleteness::Complete;
        provider.add(InvocationIncompleteReason::ProviderFailure {
            stage: InvocationDiscoveryStage::HerdrProvider,
            code: AgentFailureCode::TimedOut,
        });
        aggregate.set_filesystem(filesystem.clone());
        aggregate.set_provider(provider.clone());

        assert_eq!(
            aggregate.combined().reasons(),
            [
                InvocationIncompleteReason::RootBudget {
                    observed: 129,
                    limit: 128,
                },
                InvocationIncompleteReason::ProviderFailure {
                    stage: InvocationDiscoveryStage::HerdrProvider,
                    code: AgentFailureCode::TimedOut,
                },
            ]
        );
        aggregate.clear_provider();
        assert_eq!(aggregate.combined(), filesystem);
    }

    #[test]
    fn diagnostic_spellings_are_stable_and_content_free() {
        let reasons = [
            InvocationIncompleteReason::EntryBudget {
                observed: 2_049,
                limit: 2_048,
            },
            InvocationIncompleteReason::RegistrySize {
                observed: 524_289,
                limit: 524_288,
            },
            InvocationIncompleteReason::Cancelled {
                stage: InvocationDiscoveryStage::Filesystem,
            },
        ];
        assert_eq!(
            reasons.map(|reason| (reason.stage_code(), reason.diagnostic_code())),
            [
                ("filesystem", "entry_budget"),
                ("claude_plugin_registry", "registry_size"),
                ("filesystem", "cancelled"),
            ]
        );
    }
}
