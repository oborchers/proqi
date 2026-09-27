//! The shared launcher: installation precedence, capability gating, and exact
//! replacement by `proqi herdr toggle` or `proqi --resume`.

use super::support::{Sandbox, stderr, write_tool};

const SCRIPT: &str = "herdr-plugin/proqi.sh";
const CAPABLE: &str = "{\"data\":{\"herdr_companion_toggle\":true},\"ok\":true}";

fn fake_proqi(sandbox: &Sandbox, directory: &std::path::Path, label: &str, capabilities: &str) {
    write_tool(
        directory,
        "proqi",
        &format!(
            "if [ \"$1 $2\" = '--json capabilities' ]; then printf '%s\\n' '{capabilities}'; exit 0; fi\nprintf '{label} %s\\n' \"$*\" >> '{}'\n",
            sandbox.log().display()
        ),
    );
}

#[test]
fn toggle_replaces_itself_with_the_proqi_on_path_before_the_standalone_one() {
    let sandbox = Sandbox::new();
    sandbox.herdr();
    fake_proqi(&sandbox, &sandbox.bin(), "path", CAPABLE);
    fake_proqi(
        &sandbox,
        &sandbox.home().join(".local/bin"),
        "standalone",
        CAPABLE,
    );
    let output = sandbox.run_script(SCRIPT, &["toggle"], &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(sandbox.calls(), vec!["path herdr toggle"]);
}

#[test]
fn the_standalone_directory_is_used_when_proqi_is_missing_from_path() {
    let sandbox = Sandbox::new();
    sandbox.herdr();
    fake_proqi(
        &sandbox,
        &sandbox.home().join(".local/bin"),
        "standalone",
        CAPABLE,
    );
    let output = sandbox.run_script(SCRIPT, &["toggle"], &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(sandbox.calls(), vec!["standalone herdr toggle"]);
}

#[test]
fn a_missing_or_too_old_proqi_is_reported_through_herdr_and_not_run() {
    let missing = Sandbox::new();
    missing.herdr();
    let output = missing.run_script(SCRIPT, &["toggle"], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        missing.calls(),
        vec![
            "herdr notification show Proqi --body Proqi is not on the Herdr server's PATH. If proqi works in your shell (Homebrew or Cargo), restart the Herdr server from that shell. Otherwise run: herdr plugin install oborchers/proqi"
        ]
    );

    let old = Sandbox::new();
    old.herdr();
    fake_proqi(&old, &old.bin(), "old", "{\"data\":{},\"ok\":true}");
    let output = old.run_script(SCRIPT, &["toggle"], &[]);
    assert_eq!(output.status.code(), Some(1));
    let calls = old.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(calls[0].contains("too old for the Herdr plugin"));
    assert!(
        !calls[0].contains(&old.bin().display().to_string()),
        "no paths in notifications"
    );
}

#[test]
fn the_pane_entrypoint_resumes_exactly_the_session_it_was_given() {
    let sandbox = Sandbox::new();
    sandbox.herdr();
    fake_proqi(&sandbox, &sandbox.bin(), "path", CAPABLE);
    let session = "ses_06g30t7dv5qv55n1ppn3clis3k";
    let output = sandbox.run_script(SCRIPT, &["board"], &[("PROQI_HERDR_SESSION", session)]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(sandbox.calls(), vec![format!("path --resume {session}")]);

    let without = Sandbox::new();
    without.herdr();
    fake_proqi(&without, &without.bin(), "path", CAPABLE);
    let output = without.run_script(SCRIPT, &["board"], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(without.calls()[0].starts_with("herdr notification show Proqi"));
}

#[test]
fn an_unknown_mode_is_a_usage_error() {
    let sandbox = Sandbox::new();
    fake_proqi(&sandbox, &sandbox.bin(), "path", CAPABLE);
    let output = sandbox.run_script(SCRIPT, &["other"], &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(sandbox.calls().is_empty());
}
