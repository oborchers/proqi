//! Bounded, platform-path-backed terminal configuration loading.

use std::{fs, path::Path};

use crate::ui::UiSettings;

use super::TerminalError;

const MAX_CONFIG_BYTES: u64 = 64 * 1024;

pub(crate) fn load_settings(config_dir: &Path) -> Result<UiSettings, TerminalError> {
    let path = config_dir.join("config.toml");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(UiSettings::default());
        }
        Err(error) => return Err(TerminalError::Config(error.to_string())),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES
    {
        return Err(TerminalError::Config(
            "config.toml must be a regular file no larger than 64 KiB".to_owned(),
        ));
    }
    ensure_private(&path)?;
    let content =
        fs::read_to_string(path).map_err(|error| TerminalError::Config(error.to_string()))?;
    let settings: UiSettings =
        toml::from_str(&content).map_err(|error| TerminalError::Config(error.to_string()))?;
    settings
        .keybindings
        .validate()
        .map_err(|error| TerminalError::Config(error.to_owned()))?;
    Ok(settings)
}

#[cfg(unix)]
fn ensure_private(path: &Path) -> Result<(), TerminalError> {
    use std::os::unix::fs::PermissionsExt;
    let permissions = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, permissions).map_err(|error| TerminalError::Config(error.to_string()))
}

#[cfg(not(unix))]
fn ensure_private(_path: &Path) -> Result<(), TerminalError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::load_settings;

    #[test]
    fn missing_config_is_default_and_valid_config_is_loaded() {
        let directory = tempfile::tempdir().expect("config directory");
        assert_eq!(
            load_settings(directory.path())
                .expect("defaults")
                .keybindings
                .new,
            'n'
        );
        fs::write(
            directory.path().join("config.toml"),
            "theme = 'dark'\n[keybindings]\nnew = 't'\n",
        )
        .expect("write config");
        let settings = load_settings(directory.path()).expect("settings");
        assert_eq!(settings.keybindings.new, 't');
        assert!(settings.check_for_updates);
    }

    #[test]
    fn unknown_or_ambiguous_configuration_fails_closed() {
        let directory = tempfile::tempdir().expect("config directory");
        fs::write(directory.path().join("config.toml"), "unknown = true\n").expect("write config");
        assert!(load_settings(directory.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn configuration_symlinks_are_refused() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().expect("config directory");
        let target = directory.path().join("elsewhere.toml");
        fs::write(&target, "theme = 'auto'\n").expect("write target");
        symlink(&target, directory.path().join("config.toml")).expect("create symlink");
        assert!(load_settings(directory.path()).is_err());
    }
}
