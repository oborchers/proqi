//! Canonical assembly and global ranking of invocation-picker choices.

use crate::{
    ports::invocation::{InvocationEntry, InvocationForm},
    ui::app::BoardApp,
};

use super::{Choice, InvocationPopup, builtins, compatibility, matcher, reference};

pub(super) fn build(app: &BoardApp, popup: &InvocationPopup) -> Vec<Choice> {
    let built_ins = builtins::choices(app, popup);
    let starts_prompt = builtins::starts_prompt(app, popup);
    let mut candidates = app
        .invocation_project
        .iter()
        .chain(&app.invocation_global)
        .flat_map(|entry| entry.forms.iter().map(move |form| (entry, form)))
        .filter(|(_, form)| compatibility::supports_form(app, form))
        .filter(|(_, form)| !builtins::is_shared_starter(&form.token) || starts_prompt)
        .filter(|(_, form)| {
            !built_ins
                .iter()
                .any(|built_in| built_in.token == form.token)
        })
        .filter_map(|(entry, form)| choice_rank(entry, form, popup).map(|rank| (entry, form, rank)))
        .collect::<Vec<_>>();
    candidates.sort_by(|(left_entry, left_form, _), (right_entry, right_form, _)| {
        left_form
            .precedence
            .cmp(&right_form.precedence)
            .then_with(|| left_form.token.cmp(&right_form.token))
            .then_with(|| left_entry.kind.cmp(&right_entry.kind))
            .then_with(|| left_entry.source.cmp(&right_entry.source))
            .then_with(|| left_entry.canonical_path.cmp(&right_entry.canonical_path))
    });
    let visible = candidates.clone();
    let mut ranked = built_ins
        .into_iter()
        .chain(candidates.drain(..).map(|(entry, form, rank)| {
            let duplicate_token = visible
                .iter()
                .filter(|(_, visible_form, _)| visible_form.token == form.token)
                .count()
                > 1;
            Choice {
                token: form.token.clone(),
                insertion: form.token.clone(),
                annotation_display: None,
                separate_from_prefix: false,
                qualifier: choice_qualifier(entry, form, duplicate_token),
                qualifier_fallbacks: Vec::new(),
                group: None,
                rank,
            }
        }))
        .chain(reference::choices(app, popup))
        .collect::<Vec<_>>();
    ranked.sort_by_key(|choice| choice.rank);
    ranked
}

fn choice_rank(
    entry: &InvocationEntry,
    form: &InvocationForm,
    popup: &InvocationPopup,
) -> Option<matcher::MatchRank> {
    matcher::token(&form.token, &popup.query).or_else(|| {
        popup
            .manual
            .then_some(entry.description.as_deref())
            .flatten()
            .and_then(|description| matcher::secondary(description, &popup.query))
    })
}

fn choice_qualifier(entry: &InvocationEntry, form: &InvocationForm, show_source: bool) -> String {
    let base = format!("{} {}", entry.scope.label(), entry.kind.label());
    if show_source {
        let harness = match form.harness {
            crate::ports::invocation::InvocationHarness::ClaudeCode => "Claude",
            harness => harness.label(),
        };
        format!("{base} · {harness}")
    } else {
        base
    }
}
