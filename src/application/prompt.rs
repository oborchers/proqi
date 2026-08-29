//! Exact outbound prompt assembly independent of terminal presentation.

use crate::{
    domain::ThoughtId,
    ports::agent::{AgentTarget, CLAUDE_AGENT_KIND, CODEX_AGENT_KIND},
};

#[derive(Clone, Copy)]
pub(crate) struct SharedPromptStarter {
    pub(crate) token: &'static str,
    pub(crate) search_name: &'static str,
}

pub(crate) const SHARED_PROMPT_STARTERS: [SharedPromptStarter; 2] = [
    SharedPromptStarter {
        token: "/goal",
        search_name: "goal",
    },
    SharedPromptStarter {
        token: "/plan",
        search_name: "plan",
    },
];

pub(crate) const MULTI_THOUGHT_SEPARATOR: &str = "\n\n";

pub(crate) fn join_prompt_for_target(
    target: &AgentTarget,
    sources: &[(ThoughtId, String)],
) -> String {
    let normalize_starters = supports_shared_starters(target.agent_kind.as_str());
    sources
        .iter()
        .enumerate()
        .map(|(index, (_, content))| {
            if normalize_starters && index > 0 {
                without_later_shared_starter(content)
            } else {
                content.as_str()
            }
        })
        .collect::<Vec<_>>()
        .join(MULTI_THOUGHT_SEPARATOR)
}

pub(crate) fn supports_shared_starters(agent_kind: &str) -> bool {
    matches!(agent_kind, CODEX_AGENT_KIND | CLAUDE_AGENT_KIND)
}

fn without_later_shared_starter(content: &str) -> &str {
    let Some(starter) = SHARED_PROMPT_STARTERS
        .iter()
        .find(|starter| content.starts_with(starter.token))
    else {
        return content;
    };
    let Some(remainder) = content.get(starter.token.len()..) else {
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
    use super::without_later_shared_starter;

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
