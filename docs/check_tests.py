from __future__ import annotations

import unittest

import check


ACTION_SOURCE = '''
impl ShortcutActionId {
    pub(crate) const COMMANDS: [(Self, &str); 2] = [
        (Self::Create, "Create thought"),
        (Self::Delete, "Delete selected"),
    ];
}
'''

CLI_SOURCE = '''
struct Cli {
    #[arg(long, global = true)]
    pub(super) json: bool,
    #[arg(long, global = true, hide = true)]
    pub(super) state_dir: String,
    #[arg(short = 'c', long = "continue")]
    pub(super) continue_latest: bool,
    #[arg(short = 'r', long = "resume")]
    pub(super) resume: Option<String>,
}

enum Command {
    #[command(name = "__attachment-check", hide = true)]
    AttachmentCheckWorker,
    Capabilities,
    Sessions(SessionArgs),
}

struct SessionArgs {
    pub(super) command: Option<SessionCommand>,
}

enum SessionCommand {
    List,
}

enum UpdateCommand {
    Check,
}

enum DiagnosticsCommand {
    Collect,
}

enum ThoughtCommand {
    List,
}
'''


class CoverageTests(unittest.TestCase):
    def test_complete_fixture_is_accepted(self) -> None:
        errors, actions, surfaces = check.coverage_errors(
            ACTION_SOURCE,
            "2 actions\n**Create thought**\n**Delete selected**\n",
            CLI_SOURCE,
            "proqi\nproqi -c\nproqi --continue\nproqi -r [SESSION]\n"
            "proqi --resume [SESSION]\nproqi --json capabilities\n"
            "proqi -h\nproqi --help\nproqi -V\nproqi --version\n"
            "proqi capabilities\nproqi sessions\nproqi sessions list\n"
            "proqi update check\nproqi diagnostics collect\nproqi thoughts list\n",
        )

        self.assertEqual(errors, [])
        self.assertEqual((actions, surfaces), (2, 6))

    def test_missing_command_and_cli_surface_are_rejected(self) -> None:
        errors, _, _ = check.coverage_errors(
            ACTION_SOURCE,
            "2 actions\n**Create thought**\n",
            CLI_SOURCE,
            "proqi\nproqi -c\nproqi --continue\nproqi -r\nproqi --resume\n"
            "proqi --json capabilities\nproqi -h\nproqi --help\nproqi -V\n"
            "proqi capabilities\nproqi sessions\nproqi update check\n"
            "proqi diagnostics collect\nproqi thoughts list\n",
        )

        self.assertTrue(any("Delete selected" in error for error in errors))
        self.assertTrue(any("sessions list" in error for error in errors))
        self.assertTrue(any("--version" in error for error in errors))

    def test_missing_startup_forms_are_rejected(self) -> None:
        errors, _, _ = check.coverage_errors(
            ACTION_SOURCE,
            "2 actions\n**Create thought**\n**Delete selected**\n",
            CLI_SOURCE,
            "proqi capabilities\nproqi sessions\nproqi sessions list\n"
            "proqi update check\nproqi diagnostics collect\nproqi thoughts list\n"
            "-c --continue -r --resume --json -h --help -V --version\n",
        )

        self.assertTrue(any("plain startup" in error for error in errors))
        self.assertTrue(any("continue startup" in error for error in errors))
        self.assertTrue(any("resume startup" in error for error in errors))
        self.assertTrue(any("JSON mode" in error for error in errors))

    def test_unclosed_command_registry_is_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "closing bracket"):
            check.registered_command_labels(
                'pub(crate) const COMMANDS: [(Self, &str); 1] = [(Self::Create, "Create")'
            )


if __name__ == "__main__":
    unittest.main()
