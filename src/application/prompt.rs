//! Exact outbound prompt assembly independent of terminal presentation.

use crate::{
    domain::ThoughtId,
    ports::agent::{AgentTarget, CLAUDE_AGENT_KIND, CODEX_AGENT_KIND},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LaterThoughtPolicy {
    Preserve,
    StripStarter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SharedHarnessCommand {
    pub(crate) token: &'static str,
    pub(crate) later_thought: LaterThoughtPolicy,
}

pub(crate) const SHARED_HARNESS_COMMANDS: [SharedHarnessCommand; 19] = [
    preserved("/btw"),
    preserved("/clear"),
    preserved("/compact"),
    preserved("/diff"),
    preserved("/fast"),
    stripped("/goal"),
    preserved("/hooks"),
    preserved("/mcp"),
    preserved("/model"),
    preserved("/new"),
    preserved("/permissions"),
    stripped("/plan"),
    preserved("/rename"),
    preserved("/resume"),
    preserved("/review"),
    preserved("/skills"),
    preserved("/status"),
    preserved("/theme"),
    preserved("/usage"),
];

const fn preserved(token: &'static str) -> SharedHarnessCommand {
    SharedHarnessCommand {
        token,
        later_thought: LaterThoughtPolicy::Preserve,
    }
}

const fn stripped(token: &'static str) -> SharedHarnessCommand {
    SharedHarnessCommand {
        token,
        later_thought: LaterThoughtPolicy::StripStarter,
    }
}

pub(crate) const MULTI_THOUGHT_SEPARATOR: &str = "\n\n";

pub(crate) fn join_prompt_for_target(
    target: &AgentTarget,
    sources: &[(ThoughtId, String)],
) -> String {
    let normalize_starters = supports_shared_commands(target.agent_kind().as_str());
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

pub(crate) fn supports_shared_commands(agent_kind: &str) -> bool {
    matches!(agent_kind, CODEX_AGENT_KIND | CLAUDE_AGENT_KIND)
}

fn without_later_shared_starter(content: &str) -> &str {
    let Some(starter) =
        SHARED_HARNESS_COMMANDS
            .iter()
            .find(|command| match command.later_thought {
                LaterThoughtPolicy::Preserve => false,
                LaterThoughtPolicy::StripStarter => content.starts_with(command.token),
            })
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
    use std::collections::BTreeSet;

    use super::{LaterThoughtPolicy, SHARED_HARNESS_COMMANDS, without_later_shared_starter};

    const EXPECTED_TOKENS: [&str; 19] = [
        "/btw",
        "/clear",
        "/compact",
        "/diff",
        "/fast",
        "/goal",
        "/hooks",
        "/mcp",
        "/model",
        "/new",
        "/permissions",
        "/plan",
        "/rename",
        "/resume",
        "/review",
        "/skills",
        "/status",
        "/theme",
        "/usage",
    ];

    #[test]
    fn shared_command_inventory_and_normalization_policies_are_exact() {
        let tokens = SHARED_HARNESS_COMMANDS
            .iter()
            .map(|command| command.token)
            .collect::<Vec<_>>();
        assert_eq!(tokens, EXPECTED_TOKENS);
        assert_eq!(tokens.iter().collect::<BTreeSet<_>>().len(), tokens.len());

        for command in SHARED_HARNESS_COMMANDS {
            let expected = if matches!(command.token, "/goal" | "/plan") {
                LaterThoughtPolicy::StripStarter
            } else {
                LaterThoughtPolicy::Preserve
            };
            assert_eq!(command.later_thought, expected, "{}", command.token);
        }
    }

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

        for token in EXPECTED_TOKENS
            .into_iter()
            .filter(|token| !matches!(*token, "/plan" | "/goal"))
        {
            for content in [token.to_owned(), format!("{token} argument")] {
                assert_eq!(without_later_shared_starter(&content), content);
            }
        }
    }
}
