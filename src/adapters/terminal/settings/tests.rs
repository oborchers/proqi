use std::fs;

use ratatui_core::style::Color;

use crate::{
    ports::invocation::{InvocationHarness, InvocationKind, InvocationScope},
    ui::{BoardDensity, Theme, ThemePreference},
};

#[cfg(not(target_os = "macos"))]
use crate::ports::screenshot::ScreenshotError;

use super::{ThemeSource, load_settings};

#[test]
fn missing_config_uses_the_narrow_pane_default() {
    let directory = tempfile::tempdir().expect("config directory");
    let settings = load_settings(directory.path()).expect("defaults");
    assert_eq!(settings.ui.keybindings.new, 'n');
    assert_eq!(settings.ui.keybindings.range_up, 'K');
    assert_eq!(settings.ui.keybindings.range_down, 'J');
    assert_eq!(settings.ui.keybindings.range_select, 'v');
    assert_eq!(settings.ui.keybindings.transform, 't');
    assert_eq!(settings.ui.keybindings.screenshot_inbox, 'i');
    assert_eq!(settings.ui.keybindings.paste_reflow, 'p');
    assert_eq!(settings.ui.keybindings.delete_sentence, 'U');
    assert_eq!(settings.ui.keybindings.select_visual_row_start, 'H');
    assert_eq!(settings.ui.keybindings.select_visual_row_end, 'L');
    assert!(settings.screenshot.directory.is_none());
    assert!(settings.screenshot.filename_patterns.is_empty());
    assert!(!settings.screenshot.capture_all_new_images);
    assert_eq!(
        settings
            .screenshot
            .activity_policy()
            .inactivity_timeout_minutes(),
        20
    );
    assert_eq!(
        settings
            .screenshot
            .activity_policy()
            .max_unattended_captures(),
        10
    );
    assert!(!settings.screenshot.notify_terminal_on_auto_pause());
    assert_eq!(settings.ui.keybindings.select_all, 'a');
    assert!(!settings.ui.show_session_id);
    assert!(settings.ui.smart_lists);
    assert_eq!(settings.ui.list_indent_width, 2);
    assert_eq!(settings.ui.merge_separator, "\n\n");
    assert_eq!(settings.theme.base, ThemePreference::Auto);
    assert_eq!(settings.theme_source, ThemeSource::BuiltIn);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn screenshot_inbox_reports_truthful_platform_unsupported() {
    let directory = tempfile::tempdir().expect("config directory");
    let settings = load_settings(directory.path()).expect("defaults");

    assert_eq!(
        settings.screenshot.watcher_config(),
        Err(ScreenshotError::UnsupportedPlatform)
    );
}

#[test]
fn screenshot_inbox_settings_are_typed_bounded_and_remappable() {
    let directory = tempfile::tempdir().expect("config directory");
    let watched = directory.path().join("isolated screenshots");
    fs::create_dir(&watched).expect("watched directory");
    fs::write(
        directory.path().join("config.toml"),
        format!(
            "[keybindings]\nscreenshot_inbox = 'z'\n\
             [screenshot_inbox]\ndirectory = {:?}\n\
             filename_patterns = ['Bildschirmfoto *.png', 'Capture-??.jpg']\n\
             capture_all_new_images = true\n\
             supported_types = ['png', 'jpeg']\n\
             min_file_bytes = 128\nmax_file_bytes = 4096\n\
             max_dimension = 2048\nmax_pixels = 2000000\ndebounce_ms = 500\n\
             inactivity_timeout_minutes = 45\nmax_unattended_captures = 12\n\
             notify_terminal_on_auto_pause = true\n",
            watched.to_string_lossy(),
        ),
    )
    .expect("write config");
    let settings = load_settings(directory.path()).expect("settings");
    assert_eq!(settings.ui.keybindings.screenshot_inbox, 'z');
    assert_eq!(
        settings.screenshot.directory.as_deref(),
        Some(watched.as_path())
    );
    assert_eq!(settings.screenshot.filename_patterns.len(), 2);
    assert!(settings.screenshot.capture_all_new_images);
    assert_eq!(settings.screenshot.bounds.max_file_bytes, 4096);
    assert_eq!(settings.screenshot.debounce_ms, 500);
    assert_eq!(
        settings
            .screenshot
            .activity_policy()
            .inactivity_timeout_minutes(),
        45
    );
    assert_eq!(
        settings
            .screenshot
            .activity_policy()
            .max_unattended_captures(),
        12
    );
    assert!(settings.screenshot.notify_terminal_on_auto_pause());
}

#[test]
fn screenshot_inbox_rejects_relative_unknown_and_conflicting_configuration() {
    for config in [
        "[screenshot_inbox]\ndirectory = 'relative'\n",
        "[screenshot_inbox]\nsupported_types = ['webp']\n",
        "[screenshot_inbox]\nmax_file_bytes = 0\n",
        "[screenshot_inbox]\nunknown = true\n",
        "[screenshot_inbox]\ninactivity_timeout_minutes = 0\n",
        "[screenshot_inbox]\ninactivity_timeout_minutes = 1441\n",
        "[screenshot_inbox]\nmax_unattended_captures = 0\n",
        "[screenshot_inbox]\nmax_unattended_captures = 101\n",
        "[keybindings]\nscreenshot_inbox = 'n'\n",
    ] {
        let directory = tempfile::tempdir().expect("config directory");
        fs::write(directory.path().join("config.toml"), config).expect("config");
        assert!(load_settings(directory.path()).is_err(), "{config}");
    }
}

#[test]
fn legacy_reorder_binding_names_migrate_to_shifted_range_keys() {
    let directory = tempfile::tempdir().expect("config directory");
    fs::write(
        directory.path().join("config.toml"),
        "[keybindings]\nmove_up = 'W'\nmove_down = 'G'\n",
    )
    .expect("write config");
    let settings = load_settings(directory.path()).expect("settings");
    assert_eq!(settings.ui.keybindings.range_up, 'W');
    assert_eq!(settings.ui.keybindings.range_down, 'G');
}

#[test]
fn range_selection_latch_binding_is_remappable() {
    let directory = tempfile::tempdir().expect("config directory");
    fs::write(
        directory.path().join("config.toml"),
        "[keybindings]\nrange_select = 'b'\n",
    )
    .expect("write config");
    let settings = load_settings(directory.path()).expect("settings");
    assert_eq!(settings.ui.keybindings.range_select, 'b');
}

#[test]
fn contextual_transform_binding_is_remappable() {
    let directory = tempfile::tempdir().expect("config directory");
    fs::write(
        directory.path().join("config.toml"),
        "[keybindings]\ntransform = 'g'\n",
    )
    .expect("write config");
    let settings = load_settings(directory.path()).expect("settings");
    assert_eq!(settings.ui.keybindings.transform, 'g');
}

#[test]
fn contextual_transform_rejects_reserved_primary_bindings() {
    let directory = tempfile::tempdir().expect("config directory");
    fs::write(
        directory.path().join("config.toml"),
        "[keybindings]\ntransform = 'x'\n",
    )
    .expect("write config");
    let error = load_settings(directory.path()).expect_err("reserved transform");
    assert!(error.to_string().contains("reserved Primary shortcut"));
}

#[test]
fn whole_board_selection_binding_is_remappable() {
    let directory = tempfile::tempdir().expect("config directory");
    fs::write(
        directory.path().join("config.toml"),
        "[keybindings]\nselect_all = 'z'\n",
    )
    .expect("write config");
    let settings = load_settings(directory.path()).expect("settings");
    assert_eq!(settings.ui.keybindings.select_all, 'z');
}

#[test]
fn sentence_deletion_chord_is_remappable() {
    let directory = tempfile::tempdir().expect("config directory");
    fs::write(
        directory.path().join("config.toml"),
        "[keybindings]\ndelete_sentence = 'G'\n",
    )
    .expect("write config");
    let settings = load_settings(directory.path()).expect("settings");
    assert_eq!(settings.ui.keybindings.delete_sentence, 'G');
}

#[test]
fn sentence_deletion_rejects_unshifted_or_reserved_primary_suffixes() {
    for suffix in ['g', '1', 'A', 'Z', 'Ü'] {
        let directory = tempfile::tempdir().expect("config directory");
        fs::write(
            directory.path().join("config.toml"),
            format!("[keybindings]\ndelete_sentence = '{suffix}'\n"),
        )
        .expect("write invalid sentence binding");
        assert!(load_settings(directory.path()).is_err(), "suffix {suffix}");
    }
}

#[test]
fn visual_row_selection_fallbacks_are_remappable_and_validated() {
    let directory = tempfile::tempdir().expect("config directory");
    fs::write(
        directory.path().join("config.toml"),
        "[keybindings]\nselect_visual_row_start = 'G'\nselect_visual_row_end = 'R'\n",
    )
    .expect("write config");
    let settings = load_settings(directory.path()).expect("settings");
    assert_eq!(settings.ui.keybindings.select_visual_row_start, 'G');
    assert_eq!(settings.ui.keybindings.select_visual_row_end, 'R');

    for suffix in ['g', '1', 'A', 'Z', 'Ü'] {
        let directory = tempfile::tempdir().expect("config directory");
        fs::write(
            directory.path().join("config.toml"),
            format!("[keybindings]\nselect_visual_row_end = '{suffix}'\n"),
        )
        .expect("write invalid visual-row binding");
        assert!(load_settings(directory.path()).is_err(), "suffix {suffix}");
    }
}

#[test]
fn existing_settings_remain_compatible() {
    let directory = tempfile::tempdir().expect("config directory");
    fs::write(
        directory.path().join("config.toml"),
        "theme = 'dark'\ncheck_for_updates = false\ndensity = 'compact'\n[keybindings]\nnew = 't'\n",
    )
    .expect("write config");
    let settings = load_settings(directory.path()).expect("settings");
    assert_eq!(settings.ui.keybindings.new, 't');
    assert_eq!(settings.ui.keybindings.range_select, 'v');
    assert_eq!(settings.ui.keybindings.select_all, 'a');
    assert!(!settings.ui.check_for_updates);
    assert!(!settings.ui.show_session_id);
    assert!(settings.ui.smart_lists);
    assert_eq!(settings.ui.list_indent_width, 2);
    assert_eq!(settings.ui.density, BoardDensity::Compact);
    assert_eq!(settings.theme.base, ThemePreference::Dark);
}

#[test]
fn additional_invocation_roots_require_explicit_typed_metadata() {
    let directory = tempfile::tempdir().expect("config directory");
    fs::write(
        directory.path().join("config.toml"),
        "[[invocation_roots]]\npath = 'tooling/prompts'\nkind = 'command'\nharness = 'open_code'\nscope = 'project'\n",
    )
    .expect("write config");
    let settings = load_settings(directory.path()).expect("settings");
    assert_eq!(settings.invocation_roots.len(), 1);
    let root = &settings.invocation_roots[0];
    assert_eq!(root.kind, InvocationKind::Command);
    assert_eq!(root.harness, InvocationHarness::OpenCode);
    assert_eq!(root.scope, InvocationScope::Project);
}

#[test]
fn remote_or_plugin_scoped_additional_roots_fail_closed() {
    for (path, scope) in [
        ("https://example.com/skills", "global"),
        ("skills", "plugin"),
        ("skills", "global"),
    ] {
        let directory = tempfile::tempdir().expect("config directory");
        fs::write(
            directory.path().join("config.toml"),
            format!(
                "[[invocation_roots]]\npath = '{path}'\nkind = 'skill'\nharness = 'configured'\nscope = '{scope}'\n"
            ),
        )
        .expect("write config");
        assert!(load_settings(directory.path()).is_err());
    }
}

#[test]
fn session_identifier_visibility_is_opt_in_and_type_checked() {
    let directory = tempfile::tempdir().expect("config directory");
    fs::write(
        directory.path().join("config.toml"),
        "show_session_id = true\n",
    )
    .expect("write config");
    let settings = load_settings(directory.path()).expect("settings");
    assert!(settings.ui.show_session_id);

    fs::write(
        directory.path().join("config.toml"),
        "show_session_id = 'yes'\n",
    )
    .expect("write invalid config");
    assert!(load_settings(directory.path()).is_err());
}

#[test]
fn smart_lists_can_be_disabled_without_changing_existing_config_defaults() {
    let directory = tempfile::tempdir().expect("config directory");
    fs::write(
        directory.path().join("config.toml"),
        "smart_lists = false\n",
    )
    .expect("write config");
    let settings = load_settings(directory.path()).expect("settings");
    assert!(!settings.ui.smart_lists);
}

#[test]
fn list_indentation_width_is_configurable_and_bounded() {
    let directory = tempfile::tempdir().expect("config directory");
    fs::write(
        directory.path().join("config.toml"),
        "list_indent_width = 3\n",
    )
    .expect("write config");
    let settings = load_settings(directory.path()).expect("settings");
    assert_eq!(settings.ui.list_indent_width, 3);

    for invalid in [0, 9] {
        fs::write(
            directory.path().join("config.toml"),
            format!("list_indent_width = {invalid}\n"),
        )
        .expect("write invalid config");
        assert!(load_settings(directory.path()).is_err());
    }
}

#[test]
fn merge_separator_is_exactly_configurable_and_bounded() {
    let directory = tempfile::tempdir().expect("config directory");
    fs::write(
        directory.path().join("config.toml"),
        "merge_separator = \"\\r\\n---\\r\\n\"\n",
    )
    .expect("write config");
    let settings = load_settings(directory.path()).expect("settings");
    assert_eq!(settings.ui.merge_separator, "\r\n---\r\n");

    fs::write(
        directory.path().join("config.toml"),
        "merge_separator = ''\n",
    )
    .expect("write empty separator");
    assert!(load_settings(directory.path()).is_err());
}

#[test]
fn relative_theme_file_and_inline_precedence_are_supported() {
    let directory = tempfile::tempdir().expect("config directory");
    fs::write(
        directory.path().join("quiet.toml"),
        "schema_version = 1\nname = 'Quiet'\nbase = 'dark'\n[colors]\naccent = '#010203'\nlink = '#7DD3FC'\n",
    )
    .expect("write theme");
    fs::write(
        directory.path().join("config.toml"),
        "theme = 'quiet.toml'\n[theme_overrides]\naccent = '#70D69B'\n",
    )
    .expect("write config");
    let settings = load_settings(directory.path()).expect("settings");
    assert_eq!(settings.theme.base, ThemePreference::Dark);
    let resolved = Theme::resolve_recipe(&settings.theme, true, None).expect("theme");
    assert_eq!(resolved.accent, Color::Rgb(112, 214, 155));
    assert_eq!(resolved.link, Color::Rgb(125, 211, 252));
    assert_eq!(settings.theme_source, ThemeSource::File);
}

#[test]
fn checked_in_theme_example_resolves_through_the_public_configuration_contract() {
    let directory = tempfile::tempdir().expect("config directory");
    fs::write(
        directory.path().join("proqi-dark.toml"),
        include_str!("../../../../docs/themes/proqi-dark.toml"),
    )
    .expect("write checked-in theme");
    fs::write(
        directory.path().join("config.toml"),
        "theme = 'proqi-dark.toml'\n",
    )
    .expect("write config");
    let settings = load_settings(directory.path()).expect("settings");
    let resolved = Theme::resolve_recipe(&settings.theme, true, None).expect("accessible theme");
    assert_eq!(resolved.background, Color::Rgb(15, 13, 10));
    assert_eq!(resolved.accent, Color::Rgb(112, 214, 155));
}

#[test]
fn absolute_theme_paths_are_supported() {
    let directory = tempfile::tempdir().expect("config directory");
    let theme = directory.path().join("absolute.toml");
    fs::write(&theme, "schema_version = 1\nbase = 'dark'\n").expect("theme");
    fs::write(
        directory.path().join("config.toml"),
        format!("theme = {:?}\n", theme.to_string_lossy()),
    )
    .expect("config");
    assert!(load_settings(directory.path()).is_ok());
}

#[cfg(unix)]
#[test]
fn theme_files_may_be_symlinks_to_regular_files() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("config directory");
    let target = directory.path().join("target.toml");
    fs::write(&target, "schema_version = 1\nbase = 'light'\n").expect("theme");
    symlink(&target, directory.path().join("theme.toml")).expect("symlink");
    fs::write(
        directory.path().join("config.toml"),
        "theme = 'theme.toml'\n",
    )
    .expect("config");
    assert!(load_settings(directory.path()).is_ok());
}

#[test]
fn invalid_theme_contracts_fail_closed() {
    for theme in [
        "schema_version = 2\nbase = 'dark'\n",
        "schema_version = 1\nbase = 'limited'\n",
        "schema_version = 1\nunknown = true\n",
        "schema_version = 1\n[colors]\naccent = 'green'\n",
    ] {
        let directory = tempfile::tempdir().expect("config directory");
        fs::write(directory.path().join("theme.toml"), theme).expect("theme");
        fs::write(
            directory.path().join("config.toml"),
            "theme = 'theme.toml'\n",
        )
        .expect("config");
        assert!(load_settings(directory.path()).is_err(), "{theme}");
    }
}

#[test]
fn unknown_config_and_limited_overrides_fail_closed() {
    for config in [
        "unknown = true\n",
        "theme = 'limited'\n[theme_overrides]\naccent = '#FFFFFF'\n",
        "theme = 'https://example.com/theme.toml'\n",
    ] {
        let directory = tempfile::tempdir().expect("config directory");
        fs::write(directory.path().join("config.toml"), config).expect("config");
        assert!(load_settings(directory.path()).is_err(), "{config}");
    }
}

#[test]
fn quit_cannot_shadow_recovery_controls() {
    for key in ['r', 'w'] {
        let directory = tempfile::tempdir().expect("config directory");
        fs::write(
            directory.path().join("config.toml"),
            format!("[keybindings]\nquit = '{key}'\n"),
        )
        .expect("config");
        assert!(load_settings(directory.path()).is_err());
    }
}

#[cfg(unix)]
#[test]
fn configuration_symlinks_are_refused() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("config directory");
    let target = directory.path().join("elsewhere.toml");
    fs::write(&target, "theme = 'auto'\n").expect("target");
    symlink(&target, directory.path().join("config.toml")).expect("symlink");
    assert!(load_settings(directory.path()).is_err());
}
