//! Evidence-backed invocation token projection for one discovered definition.

use crate::ports::invocation::{InvocationForm, InvocationHarness, InvocationKind};

use super::ScanRoot;

pub(super) fn for_entry(root: &ScanRoot, name: &str) -> Vec<InvocationForm> {
    if !root.insertable {
        return Vec::new();
    }
    let token = match (root.harness, root.kind) {
        (InvocationHarness::Codex | InvocationHarness::AgentSkills, InvocationKind::Skill) => {
            Some(format!("${name}"))
        }
        (InvocationHarness::ClaudeCode, InvocationKind::Skill | InvocationKind::Command) => Some(
            root.plugin
                .as_ref()
                .map_or_else(|| format!("/{name}"), |plugin| format!("/{plugin}:{name}")),
        ),
        (InvocationHarness::ClaudeCode, InvocationKind::Agent) => {
            Some(root.plugin.as_ref().map_or_else(
                || format!("@agent-{name}"),
                |plugin| format!("@agent-{plugin}:{name}"),
            ))
        }
        (
            InvocationHarness::OpenCode | InvocationHarness::Pi | InvocationHarness::Configured,
            InvocationKind::Command,
        ) => Some(format!("/{name}")),
        (InvocationHarness::OpenCode, InvocationKind::Agent) => Some(format!("@{name}")),
        (InvocationHarness::Pi, InvocationKind::Skill) => Some(format!("/skill:{name}")),
        (InvocationHarness::Configured, InvocationKind::Skill) => Some(format!("${name}")),
        _ => None,
    };
    token
        .map(|token| {
            vec![InvocationForm {
                harness: if root.harness == InvocationHarness::AgentSkills {
                    InvocationHarness::Codex
                } else {
                    root.harness
                },
                token,
                precedence: root.precedence,
            }]
        })
        .unwrap_or_default()
}
