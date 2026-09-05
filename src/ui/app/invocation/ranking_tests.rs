//! Cross-picker fuzzy ranking, compatibility, and scale contracts.

use std::path::PathBuf;

use super::{
    matcher,
    tests::contract::{app, entry, install, target},
};
use crate::{
    application::Effect,
    ports::{
        agent::{CLAUDE_AGENT_KIND, CODEX_AGENT_KIND, OPENCODE_AGENT_KIND},
        invocation::{
            InvocationCompleteness, InvocationDiscovery, InvocationForm, InvocationHarness,
            InvocationIncompleteReason, InvocationKind, InvocationScope,
        },
    },
    ui::{UiInput, UiKey},
};

fn tokens(app: &crate::ui::BoardApp) -> Vec<String> {
    app.invocation_view()
        .expect("invocation picker")
        .1
        .into_iter()
        .map(|choice| choice.token)
        .collect()
}

#[test]
fn picker_orders_exact_prefix_contiguous_boundary_and_sparse_token_matches() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("/ace", cwd.path());
    install(
        &mut app,
        cwd.path(),
        [
            "/a-very-long-circuitous-extended",
            "/ace-tools",
            "/trace",
            "/aos-communication-email",
            "/ace",
        ]
        .into_iter()
        .map(|token| entry(token, InvocationKind::Command, InvocationScope::Project))
        .collect(),
    );
    app.refresh_invocation_popup();

    assert_eq!(
        tokens(&app),
        [
            "/ace",
            "/ace-tools",
            "/trace",
            "/aos-communication-email",
            "/a-very-long-circuitous-extended",
        ]
    );
}

#[test]
fn short_aos_email_abbreviation_completes_and_redoes_as_one_exact_edit() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("$aos-ce", cwd.path());
    install(
        &mut app,
        cwd.path(),
        vec![entry(
            "$aos-communication-email",
            InvocationKind::Skill,
            InvocationScope::Global,
        )],
    );
    app.refresh_invocation_popup();

    assert_eq!(tokens(&app), ["$aos-communication-email"]);
    app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    assert_eq!(
        app.editor_snapshot().expect("editor").content,
        "$aos-communication-email "
    );
    app.handle(UiInput::Key(UiKey::Undo), &mut ids, &clock);
    assert_eq!(app.editor_snapshot().expect("editor").content, "$aos-ce");
    app.handle(UiInput::Key(UiKey::Redo), &mut ids, &clock);
    assert_eq!(
        app.editor_snapshot().expect("editor").content,
        "$aos-communication-email "
    );
}

#[test]
fn equal_fuzzy_scores_defer_to_existing_precedence_and_canonical_order() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("/ace", cwd.path());
    let mut later = entry(
        "/aos-communication-email",
        InvocationKind::Agent,
        InvocationScope::Plugin,
    );
    later.forms[0].precedence = 30;
    later.canonical_path = PathBuf::from("/z/later");
    let mut earlier = later.clone();
    earlier.kind = InvocationKind::Command;
    earlier.scope = InvocationScope::Project;
    earlier.forms[0].precedence = 5;
    earlier.canonical_path = PathBuf::from("/z/earlier");
    let mut lexical = earlier.clone();
    lexical.kind = InvocationKind::Skill;
    lexical.canonical_path = PathBuf::from("/a/first");
    install(&mut app, cwd.path(), vec![later, earlier, lexical]);
    app.refresh_invocation_popup();

    let qualifiers = app
        .invocation_view()
        .expect("picker")
        .1
        .into_iter()
        .map(|choice| choice.qualifier)
        .collect::<Vec<_>>();
    assert_eq!(
        qualifiers,
        [
            "Project Skill · Codex",
            "Project Command · Codex",
            "Plugin Agent · Codex"
        ]
    );
}

#[test]
fn fuzzy_forms_retain_sigil_and_target_compatibility() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let forms = [
        (InvocationHarness::Codex, "$aos-communication-email"),
        (InvocationHarness::ClaudeCode, "/aos-communication-email"),
        (InvocationHarness::OpenCode, "@aos-communication-email"),
    ];
    for (target_kind, query, expected) in [
        (CODEX_AGENT_KIND, "$aos-ce", "$aos-communication-email"),
        (CLAUDE_AGENT_KIND, "/aos-ce", "/aos-communication-email"),
        (OPENCODE_AGENT_KIND, "@aos-ce", "@aos-communication-email"),
    ] {
        let (mut app, _, _) = app(query, cwd.path());
        app.complete_agent_discovery(Ok(vec![target(target_kind)]));
        let mut definition = entry(
            "$placeholder",
            InvocationKind::Skill,
            InvocationScope::Project,
        );
        definition.forms = forms
            .into_iter()
            .map(|(harness, token)| InvocationForm {
                harness,
                token: token.to_owned(),
                precedence: 10,
            })
            .collect();
        install(&mut app, cwd.path(), vec![definition]);
        app.refresh_invocation_popup();
        assert_eq!(tokens(&app), [expected]);
    }
}

#[test]
fn unicode_case_and_combining_queries_rank_the_canonical_token() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("/CAFE\u{301}-T", cwd.path());
    install(
        &mut app,
        cwd.path(),
        vec![
            entry(
                "/cafe\u{301}-tools",
                InvocationKind::Command,
                InvocationScope::Project,
            ),
            entry(
                "/café-triage",
                InvocationKind::Command,
                InvocationScope::Project,
            ),
        ],
    );
    app.refresh_invocation_popup();
    assert_eq!(tokens(&app), ["/cafe\u{301}-tools", "/café-triage"]);
}

#[test]
fn manual_description_matches_stay_below_every_token_match() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("text", cwd.path());
    let token = entry(
        "/media-image",
        InvocationKind::Skill,
        InvocationScope::Project,
    );
    let mut prose = entry(
        "/unrelated",
        InvocationKind::Skill,
        InvocationScope::Project,
    );
    prose.description = Some("Exact image".to_owned());
    install(&mut app, cwd.path(), vec![prose, token]);
    app.open_invocation_picker();
    app.handle(UiInput::Paste("image".to_owned()), &mut ids, &clock);
    assert_eq!(tokens(&app), ["/media-image", "/unrelated"]);
}

#[test]
fn source_and_scope_labels_do_not_participate_in_manual_search() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("text", cwd.path());
    install(
        &mut app,
        cwd.path(),
        vec![entry(
            "$ordinary",
            InvocationKind::Skill,
            InvocationScope::Global,
        )],
    );
    app.open_invocation_picker();
    app.handle(UiInput::Paste("Global".to_owned()), &mut ids, &clock);

    assert!(tokens(&app).is_empty());
}

#[test]
fn empty_manual_query_one_match_and_no_automatic_match_are_distinct() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut manual, mut ids, clock) = app("text", cwd.path());
    install(
        &mut manual,
        cwd.path(),
        vec![entry(
            "$only",
            InvocationKind::Skill,
            InvocationScope::Project,
        )],
    );
    manual.open_invocation_picker();
    assert_eq!(tokens(&manual), ["$only"]);
    manual.handle(UiInput::Paste("absent".to_owned()), &mut ids, &clock);
    assert!(tokens(&manual).is_empty());

    let (mut automatic, _, _) = app("$absent", cwd.path());
    install(
        &mut automatic,
        cwd.path(),
        vec![entry(
            "$only",
            InvocationKind::Skill,
            InvocationScope::Project,
        )],
    );
    automatic.refresh_invocation_popup();
    assert!(automatic.invocation_view().is_none());
}

#[test]
fn rapid_typing_backspace_and_paste_recompute_without_losing_editor_text() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("$a", cwd.path());
    install(
        &mut app,
        cwd.path(),
        vec![
            entry("$archive", InvocationKind::Skill, InvocationScope::Project),
            entry(
                "$aos-communication-email",
                InvocationKind::Skill,
                InvocationScope::Project,
            ),
        ],
    );
    app.refresh_invocation_popup();
    app.handle(UiInput::Key(UiKey::Character('o')), &mut ids, &clock);
    app.handle(UiInput::Paste("s-c".to_owned()), &mut ids, &clock);
    app.handle(UiInput::Key(UiKey::Backspace), &mut ids, &clock);
    app.handle(UiInput::Key(UiKey::Character('c')), &mut ids, &clock);
    app.handle(UiInput::Key(UiKey::Character('e')), &mut ids, &clock);

    assert_eq!(app.editor_snapshot().expect("editor").content, "$aos-ce");
    assert_eq!(tokens(&app), ["$aos-communication-email"]);
}

#[test]
fn overflow_and_incompleteness_remain_simultaneously_true() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("$cz", cwd.path());
    let effects = app.refresh_invocations();
    let [Effect::DiscoverInvocations(request)] = effects.as_slice() else {
        panic!("discovery request");
    };
    let mut completeness = InvocationCompleteness::Complete;
    completeness.add(InvocationIncompleteReason::EntryBudget {
        observed: 2_049,
        limit: 2_048,
    });
    app.complete_invocation_discovery(InvocationDiscovery {
        generation: request.generation,
        cwd: cwd.path().to_owned(),
        global: Vec::new(),
        project: (0..25)
            .map(|index| {
                entry(
                    &format!("$catalog-{index:02}-zoom"),
                    InvocationKind::Skill,
                    InvocationScope::Project,
                )
            })
            .collect(),
        completeness,
    });
    app.refresh_invocation_popup();

    assert_eq!(app.invocation_match_count(), 25);
    assert_eq!(
        app.invocation_notice(),
        Some(" incomplete results, refine query ")
    );
    assert_eq!(app.invocation_overflow(5), (false, true));
}

#[test]
fn maximum_valid_catalog_is_ranked_without_silent_truncation() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("$cz", cwd.path());
    install(
        &mut app,
        cwd.path(),
        (0..2_048)
            .map(|index| {
                entry(
                    &format!("$catalog-{index:04}-zoom-zone"),
                    InvocationKind::Skill,
                    InvocationScope::Project,
                )
            })
            .collect(),
    );
    matcher::reset_rank_call_count();
    app.refresh_invocation_popup();
    let ranked_once = matcher::rank_call_count();
    assert_eq!(app.invocation_match_count(), 2_048);
    assert_eq!(
        app.invocation_notice(),
        Some(" more results exist, refine query ")
    );
    let _view = app.invocation_view();
    let _overflow = app.invocation_overflow(12);
    app.prepare_frame(ratatui_core::layout::Rect::new(0, 0, 80, 24));
    assert_eq!(matcher::rank_call_count(), ranked_once);
}
