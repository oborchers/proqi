//! Exact outbound prompt assembly with shared starter normalization.

use crate::{
    domain::ThoughtId,
    ports::agent::{AgentTarget, CLAUDE_AGENT_KIND, CODEX_AGENT_KIND},
    ui::app::invocation::builtins,
};

pub(super) fn join_for_target(target: &AgentTarget, sources: &[(ThoughtId, String)]) -> String {
    let normalize_starters = matches!(
        target.agent_kind.as_str(),
        CODEX_AGENT_KIND | CLAUDE_AGENT_KIND
    );
    sources
        .iter()
        .enumerate()
        .map(|(index, (_, content))| {
            if normalize_starters && index > 0 {
                builtins::without_later_shared_starter(content)
            } else {
                content.as_str()
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use crate::ui::app::invocation::builtins::without_later_shared_starter;

    #[test]
    fn strips_only_complete_shared_starters_and_one_separator() {
        for token in ["/plan", "/goal"] {
            for (suffix, expected) in [
                ("", ""),
                (" task", "task"),
                ("\ntask", "task"),
                ("\r\ntask", "task"),
                ("\n\ntask", "\ntask"),
            ] {
                let content = format!("{token}{suffix}");
                assert_eq!(without_later_shared_starter(&content), expected);
            }
            let partial = format!("{token}ner task");
            assert_eq!(without_later_shared_starter(&partial), partial);
            let in_body = format!("text {token} task");
            assert_eq!(without_later_shared_starter(&in_body), in_body);
        }
    }
}
