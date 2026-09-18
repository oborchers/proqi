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
enum Command {
    Capabilities,
    Sessions(SessionArgs),
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
            "proqi capabilities\nproqi sessions list\nproqi update check\n"
            "proqi diagnostics collect\nproqi thoughts list\n",
        )

        self.assertEqual(errors, [])
        self.assertEqual((actions, surfaces), (2, 5))

    def test_missing_command_and_cli_surface_are_rejected(self) -> None:
        errors, _, _ = check.coverage_errors(
            ACTION_SOURCE,
            "2 actions\n**Create thought**\n",
            CLI_SOURCE,
            "proqi capabilities\nproqi update check\nproqi diagnostics collect\n"
            "proqi thoughts list\n",
        )

        self.assertTrue(any("Delete selected" in error for error in errors))
        self.assertTrue(any("sessions list" in error for error in errors))

    def test_unclosed_command_registry_is_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "closing bracket"):
            check.registered_command_labels(
                'pub(crate) const COMMANDS: [(Self, &str); 1] = [(Self::Create, "Create")'
            )


if __name__ == "__main__":
    unittest.main()
