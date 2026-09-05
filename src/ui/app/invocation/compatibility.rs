//! Target-aware filtering for exact invocation forms.

use std::collections::BTreeSet;

use crate::ports::{
    agent::{CLAUDE_AGENT_KIND, CODEX_AGENT_KIND, OPENCODE_AGENT_KIND, PI_AGENT_KIND},
    invocation::{InvocationForm, InvocationHarness},
};

use crate::ui::app::BoardApp;

pub(super) fn supports_form(app: &BoardApp, form: &InvocationForm) -> bool {
    let targets = target_harnesses(app);
    targets.is_empty() || targets.contains(&form.harness)
}

fn target_harnesses(app: &BoardApp) -> BTreeSet<InvocationHarness> {
    app.agent_targets()
        .iter()
        .filter(|target| target.delivery.supports())
        .filter_map(|target| match target.agent_kind().as_str() {
            CODEX_AGENT_KIND => Some(InvocationHarness::Codex),
            CLAUDE_AGENT_KIND => Some(InvocationHarness::ClaudeCode),
            OPENCODE_AGENT_KIND => Some(InvocationHarness::OpenCode),
            PI_AGENT_KIND => Some(InvocationHarness::Pi),
            _ => None,
        })
        .collect()
}
