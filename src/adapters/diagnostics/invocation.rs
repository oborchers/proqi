//! Content-redacted projection for incomplete invocation discovery.

use crate::ports::invocation::{
    InvocationIncompleteReason,
    InvocationIncompleteReason::{
        Cancelled, EntryBudget, ManifestSize, PathBudget, ProviderFailure, ProviderRowBudget,
        RecursiveDepth, RegistrySize, RootBudget,
    },
};

#[derive(Debug, Eq, PartialEq)]
struct Fields {
    stage: &'static str,
    reason: &'static str,
    observed: Option<u64>,
    limit: Option<u64>,
    affected: Option<usize>,
    provider_code: Option<&'static str>,
}

pub(super) fn record(reason: &InvocationIncompleteReason) {
    let fields = fields(reason);
    tracing::warn!(
        event = "invocation_discovery_incomplete",
        stage = fields.stage,
        reason = fields.reason,
        observed = fields.observed,
        limit = fields.limit,
        affected = fields.affected,
        provider_code = fields.provider_code
    );
}

fn fields(reason: &InvocationIncompleteReason) -> Fields {
    let (observed, limit, affected, provider_code) = match reason {
        EntryBudget { observed, limit }
        | PathBudget { observed, limit }
        | RootBudget { observed, limit }
        | RecursiveDepth { observed, limit }
        | ProviderRowBudget {
            observed, limit, ..
        } => (
            Some(u64::try_from(*observed).unwrap_or(u64::MAX)),
            Some(u64::try_from(*limit).unwrap_or(u64::MAX)),
            None,
            None,
        ),
        RegistrySize { observed, limit } => (Some(*observed), Some(*limit), None, None),
        ManifestSize {
            observed,
            limit,
            affected,
        } => (Some(*observed), Some(*limit), Some(*affected), None),
        ProviderFailure { code, .. } => (None, None, None, Some(code.as_str())),
        Cancelled { .. } => (None, None, None, None),
    };
    Fields {
        stage: reason.stage_code(),
        reason: reason.diagnostic_code(),
        observed,
        limit,
        affected,
        provider_code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{agent::AgentFailureCode, invocation::InvocationDiscoveryStage};

    #[test]
    fn stable_projection_contains_only_codes_and_aggregate_counts() {
        assert_eq!(
            fields(&ManifestSize {
                observed: 65_537,
                limit: 65_536,
                affected: 3,
            }),
            Fields {
                stage: "claude_plugin_manifest",
                reason: "manifest_size",
                observed: Some(65_537),
                limit: Some(65_536),
                affected: Some(3),
                provider_code: None,
            }
        );
        assert_eq!(
            fields(&ProviderFailure {
                stage: InvocationDiscoveryStage::HerdrProvider,
                code: AgentFailureCode::TimedOut,
            }),
            Fields {
                stage: "herdr_provider",
                reason: "provider_failure",
                observed: None,
                limit: None,
                affected: None,
                provider_code: Some("timed_out"),
            }
        );
    }

    #[test]
    fn projection_type_has_no_content_or_path_field() {
        let rendered = format!(
            "{:?}",
            fields(&EntryBudget {
                observed: 2_049,
                limit: 2_048,
            })
        );
        assert_eq!(
            rendered,
            "Fields { stage: \"filesystem\", reason: \"entry_budget\", observed: Some(2049), limit: Some(2048), affected: None, provider_code: None }"
        );
    }
}
