//! Disposable directories and fake executables for Herdr plugin process tests.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// Repository-relative plugin file.
pub fn plugin_file(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

/// One isolated HOME, fake-tool directory, and call log.
pub struct Sandbox {
    root: tempfile::TempDir,
}

impl Sandbox {
    pub fn new() -> Self {
        let sandbox = Self {
            root: tempfile::tempdir().expect("sandbox"),
        };
        fs::create_dir_all(sandbox.bin()).expect("fake bin");
        fs::create_dir_all(sandbox.home()).expect("home");
        sandbox
    }

    pub fn path(&self) -> &Path {
        self.root.path()
    }

    pub fn bin(&self) -> PathBuf {
        self.path().join("bin")
    }

    pub fn home(&self) -> PathBuf {
        self.path().join("home")
    }

    pub fn log(&self) -> PathBuf {
        self.path().join("calls.log")
    }

    /// Lines recorded by fake tools, in call order.
    pub fn calls(&self) -> Vec<String> {
        fs::read_to_string(self.log())
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Fake Herdr that records every call and answers notifications.
    pub fn herdr(&self) -> PathBuf {
        let log = self.log();
        write_tool(
            &self.bin(),
            "herdr",
            &format!("printf 'herdr %s\\n' \"$*\" >> '{}'\n", log.display()),
        )
    }

    /// Run one plugin script with only the sandbox and system tools on PATH.
    pub fn run_script(&self, script: &str, arguments: &[&str], extra: &[(&str, &str)]) -> Output {
        let mut command = Command::new("/bin/sh");
        command
            .arg(plugin_file(script))
            .args(arguments)
            .env_clear()
            .env("PATH", format!("{}:/usr/bin:/bin", self.bin().display()))
            .env("HOME", self.home())
            .env("TMPDIR", self.path())
            .env("HERDR_BIN_PATH", self.bin().join("herdr"));
        for (key, value) in extra {
            command.env(key, value);
        }
        command.output().expect("run plugin script")
    }
}

/// Install an executable POSIX shell script.
pub fn write_tool(directory: &Path, name: &str, body: &str) -> PathBuf {
    fs::create_dir_all(directory).expect("tool directory");
    let path = directory.join(name);
    let source = directory.join(format!(".{name}.source"));
    fs::write(&source, format!("#!/bin/sh\n{body}")).expect("tool source");
    // A separate process writes the executable. Under `cargo test`, a test
    // thread that forks while this process still holds a write handle to the
    // tool would make executing it fail with ETXTBSY on Linux.
    let installed = Command::new("install")
        .args(["-m", "755"])
        .arg(&source)
        .arg(&path)
        .status()
        .expect("run install");
    assert!(installed.success(), "install {}", path.display());
    fs::remove_file(&source).expect("remove tool source");
    path
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}
