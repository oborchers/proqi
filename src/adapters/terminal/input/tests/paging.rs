use super::*;

#[test]
fn alt_arrows_and_page_keys_share_one_normalized_fast_intention() {
    assert_eq!(
        translate_in(
            ShortcutContext::Edit,
            Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT)),
        ),
        Some(UiInput::Key(UiKey::FastNavigation {
            direction: FastNavigation::Previous,
            extend_selection: false,
        }))
    );
    assert_eq!(
        translate_in(
            ShortcutContext::Edit,
            Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT)),
        ),
        Some(UiInput::Key(UiKey::FastNavigation {
            direction: FastNavigation::Next,
            extend_selection: false,
        }))
    );
    for (code, direction) in [
        (KeyCode::PageUp, FastNavigation::Previous),
        (KeyCode::PageDown, FastNavigation::Next),
    ] {
        assert_eq!(
            translate_in(
                ShortcutContext::Edit,
                Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
            ),
            Some(UiInput::Key(UiKey::FastNavigation {
                direction,
                extend_selection: false,
            }))
        );
    }
}

#[test]
fn shifted_fast_spellings_extend_the_same_five_row_selection() {
    for code in [KeyCode::Down, KeyCode::PageDown] {
        let modifiers = if code == KeyCode::Down {
            KeyModifiers::ALT | KeyModifiers::SHIFT
        } else {
            KeyModifiers::SHIFT
        };
        assert_eq!(
            translate_in(
                ShortcutContext::Edit,
                Event::Key(KeyEvent::new(code, modifiers)),
            ),
            Some(UiInput::Key(UiKey::FastNavigation {
                direction: FastNavigation::Next,
                extend_selection: true,
            }))
        );
    }
}
