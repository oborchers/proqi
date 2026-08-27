use super::*;

const WIDTH: u16 = 40;
const HEIGHT: u16 = 8;

fn text_origin(fixture: &mut Fixture) -> (u16, u16) {
    let _frame = draw(fixture, WIDTH, HEIGHT);
    let area = fixture
        .app
        .prepare_frame(Rect::new(0, 0, WIDTH, HEIGHT))
        .thoughts[0]
        .text_area;
    (area.x, area.y)
}

fn click(fixture: &mut Fixture, column: u16, row: u16, shifted: bool) {
    pointer(
        fixture,
        column,
        row,
        PointerKind::Down(PointerButton::Left),
        shifted,
    );
    let _pressed = draw(fixture, WIDTH, HEIGHT);
    pointer(
        fixture,
        column,
        row,
        PointerKind::Up(PointerButton::Left),
        shifted,
    );
    let _released = draw(fixture, WIDTH, HEIGHT);
}

fn pointer(
    fixture: &mut Fixture,
    column: u16,
    row: u16,
    kind: PointerKind,
    extend_selection: bool,
) {
    fixture.input(UiInput::Pointer(PointerInput {
        column,
        row,
        kind,
        extend_selection,
    }));
}

#[test]
fn double_and_triple_click_select_word_then_logical_line() {
    let mut fixture = Fixture::new();
    fixture.paste("alpha beta\ngamma delta");
    let (x, y) = text_origin(&mut fixture);
    let beta = x + 7;

    click(&mut fixture, beta, y, false);
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("single click")
            .selection
            .is_none()
    );

    click(&mut fixture, beta, y, false);
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("double click")
            .selection,
        Some(proqi::ports::editor::TextSelection {
            start: proqi::domain::TextPosition::new(0, 6),
            end: proqi::domain::TextPosition::new(0, 10),
        })
    );

    click(&mut fixture, beta, y, false);
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("triple click")
            .selection,
        Some(proqi::ports::editor::TextSelection {
            start: proqi::domain::TextPosition::new(0, 0),
            end: proqi::domain::TextPosition::new(1, 0),
        })
    );

    click(&mut fixture, beta, y, false);
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("fourth click")
            .selection
            .is_none()
    );
}

#[test]
fn click_streak_requires_time_position_and_forward_clock_progress() {
    let mut fixture = Fixture::new();
    fixture.paste("alpha beta gamma");
    let (x, y) = text_origin(&mut fixture);

    click(&mut fixture, x + 1, y, false);
    fixture.clock.set(Timestamp::from_millis(521));
    click(&mut fixture, x + 1, y, false);
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("expired click")
            .selection
            .is_none()
    );

    fixture.clock.set(Timestamp::from_millis(500));
    click(&mut fixture, x + 1, y, false);
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("backward clock")
            .selection
            .is_none()
    );

    fixture.clock.set(Timestamp::from_millis(600));
    click(&mut fixture, x + 1, y, false);
    click(&mut fixture, x + 4, y, false);
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("distant click")
            .selection
            .is_none()
    );

    fixture.clock.set(Timestamp::from_millis(1_100));
    click(&mut fixture, x + 4, y, false);
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("threshold click")
            .selection,
        Some(proqi::ports::editor::TextSelection {
            start: proqi::domain::TextPosition::new(0, 0),
            end: proqi::domain::TextPosition::new(0, 5),
        })
    );
}

#[test]
fn double_click_drag_extends_by_complete_words_across_lines() {
    let mut fixture = Fixture::new();
    fixture.paste("alpha beta\ngamma delta");
    let (x, y) = text_origin(&mut fixture);
    let beta = x + 7;

    click(&mut fixture, beta, y, false);
    fixture.pointer(beta, y, PointerKind::Down(PointerButton::Left));
    let _selected = draw(&mut fixture, WIDTH, HEIGHT);
    fixture.pointer(x + 2, y + 1, PointerKind::Drag(PointerButton::Left));

    assert_eq!(
        fixture.app.editor_snapshot().expect("word drag").selection,
        Some(proqi::ports::editor::TextSelection {
            start: proqi::domain::TextPosition::new(0, 6),
            end: proqi::domain::TextPosition::new(1, 5),
        })
    );
    fixture.pointer(x + 2, y + 1, PointerKind::Up(PointerButton::Left));
}

#[test]
fn shifted_double_click_extends_the_existing_selection_by_words() {
    let mut fixture = Fixture::new();
    fixture.paste("alpha beta\ngamma delta");
    let (x, y) = text_origin(&mut fixture);
    let beta = x + 7;

    click(&mut fixture, beta, y, false);
    click(&mut fixture, beta, y, false);
    fixture.clock.set(Timestamp::from_millis(600));
    click(&mut fixture, x + 2, y + 1, true);
    click(&mut fixture, x + 2, y + 1, true);

    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("shifted word")
            .selection,
        Some(proqi::ports::editor::TextSelection {
            start: proqi::domain::TextPosition::new(0, 6),
            end: proqi::domain::TextPosition::new(1, 5),
        })
    );
}

#[test]
fn triple_click_drag_extends_by_complete_logical_lines() {
    let mut fixture = Fixture::new();
    fixture.paste("alpha beta\ngamma delta\nomega");
    let (x, y) = text_origin(&mut fixture);
    let beta = x + 7;

    click(&mut fixture, beta, y, false);
    click(&mut fixture, beta, y, false);
    pointer(
        &mut fixture,
        beta,
        y,
        PointerKind::Down(PointerButton::Left),
        false,
    );
    let _selected = draw(&mut fixture, WIDTH, HEIGHT);
    fixture.pointer(x + 2, y + 1, PointerKind::Drag(PointerButton::Left));

    assert_eq!(
        fixture.app.editor_snapshot().expect("line drag").selection,
        Some(proqi::ports::editor::TextSelection {
            start: proqi::domain::TextPosition::new(0, 0),
            end: proqi::domain::TextPosition::new(2, 0),
        })
    );
}
