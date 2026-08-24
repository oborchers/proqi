use super::*;

#[test]
fn storage_failure_blocks_new_edits_and_exposes_retry() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste("durable candidate");
    fixture.app.acknowledge_persistence(sequence, false);
    let before = fixture.app.editor_snapshot().expect("editor");
    assert!(
        fixture
            .effects(UiInput::Key(UiKey::Character('x')))
            .is_empty()
    );
    assert_eq!(fixture.app.editor_snapshot().expect("editor"), before);
    assert_eq!(
        fixture.effects(UiInput::Key(UiKey::Character('r'))),
        vec![Effect::RetryPersistence { sequence }]
    );
}

#[test]
fn typing_coalesces_until_a_semantic_boundary() {
    let mut fixture = Fixture::new();
    fixture.paste("");
    for character in "hello".chars() {
        assert!(
            fixture
                .effects(UiInput::Key(UiKey::Character(character)))
                .is_empty()
        );
    }
    assert!(fixture.app.has_pending_edit());
    let terminal = draw(&mut fixture, 40, 8);
    assert!(text(terminal.backend().buffer()).contains("edit · saving"));
    let effects = fixture
        .app
        .flush_pending_edit(&mut fixture.ids, &fixture.clock);
    assert_eq!(effects.len(), 1);
    let Effect::CommitRevision(revision) = &effects[0] else {
        panic!("expected one coalesced revision");
    };
    assert_eq!(revision.before_content, "");
    assert_eq!(revision.after_content, "hello");
    assert!(!fixture.app.has_pending_edit());
}

#[test]
fn a_save_failure_cancels_a_requested_exit() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste("must survive");
    fixture.input(UiInput::Key(UiKey::Quit));
    assert!(fixture.app.quit);
    fixture.app.acknowledge_persistence(sequence, false);
    assert!(!fixture.app.quit);
}

#[test]
fn successful_retry_rearms_an_unsaved_editor_buffer() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste("base");
    fixture.input(UiInput::Key(UiKey::Character('x')));
    let generation = fixture.app.edit_generation();
    fixture.app.acknowledge_persistence(sequence, false);
    assert_eq!(
        fixture.effects(UiInput::Key(UiKey::Character('r'))),
        vec![Effect::RetryPersistence { sequence }]
    );
    fixture.app.acknowledge_persistence(sequence, true);
    assert!(fixture.app.edit_generation() > generation);
    assert!(fixture.app.has_pending_edit());
    assert_eq!(
        fixture
            .app
            .flush_pending_edit(&mut fixture.ids, &fixture.clock)
            .len(),
        1
    );
}
