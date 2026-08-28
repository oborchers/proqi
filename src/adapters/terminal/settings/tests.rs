use std::fs;

use ratatui_core::style::Color;

use crate::ui::{BoardDensity, Theme, ThemePreference};

use super::{ThemeSource, load_settings};

#[test]
fn missing_config_uses_the_adaptive_default() {
    let directory = tempfile::tempdir().expect("config directory");
    let settings = load_settings(directory.path()).expect("defaults");
    assert_eq!(settings.ui.keybindings.new, 'n');
    assert!(!settings.ui.show_session_id);
    assert!(settings.ui.smart_lists);
    assert_eq!(settings.theme.base, ThemePreference::Auto);
    assert_eq!(settings.theme_source, ThemeSource::BuiltIn);
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
    assert!(!settings.ui.check_for_updates);
    assert!(!settings.ui.show_session_id);
    assert!(settings.ui.smart_lists);
    assert_eq!(settings.ui.density, BoardDensity::Compact);
    assert_eq!(settings.theme.base, ThemePreference::Dark);
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
