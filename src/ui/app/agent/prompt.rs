//! Exact outbound prompt assembly with the narrow plan-mode exception.

use crate::{
    domain::ThoughtId,
    ports::agent::{AgentTarget, CLAUDE_AGENT_KIND, CODEX_AGENT_KIND},
};

pub(super) fn join_for_target(target: &AgentTarget, sources: &[(ThoughtId, String)]) -> String {
    let normalize_plan = matches!(
        target.agent_kind.as_str(),
        CODEX_AGENT_KIND | CLAUDE_AGENT_KIND
    );
    sources
        .iter()
        .enumerate()
        .map(|(index, (_, content))| {
            if normalize_plan && index > 0 {
                without_leading_plan(content)
            } else {
                content.as_str()
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn without_leading_plan(content: &str) -> &str {
    let Some(remainder) = content.strip_prefix("/plan") else {
        return content;
    };
    let Some(separator) = remainder.chars().next() else {
        return remainder;
    };
    if !separator.is_whitespace() {
        return content;
    }
    let separator_len = if remainder.starts_with("\r\n") {
        2
    } else {
        separator.len_utf8()
    };
    &remainder[separator_len..]
}

#[cfg(test)]
mod tests {
    use super::without_leading_plan;

    #[test]
    fn strips_only_a_complete_leading_plan_token_and_one_separator() {
        for (content, expected) in [
            ("/plan", ""),
            ("/plan task", "task"),
            ("/plan\ntask", "task"),
            ("/plan\r\ntask", "task"),
            ("/plan\n\ntask", "\ntask"),
            ("/planner task", "/planner task"),
            ("text /plan task", "text /plan task"),
        ] {
            assert_eq!(without_leading_plan(content), expected);
        }
    }
}
