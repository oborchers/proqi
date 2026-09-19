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
    Update(UpdateArgs),
    Diagnostics(DiagnosticsArgs),
    Sessions(SessionArgs),
    Items(ItemArgs),
    Thoughts(ThoughtArgs),
}

struct SessionArgs {
    #[command(subcommand)]
    pub(super) command: Option<SessionCommand>,
}

enum SessionCommand {
    List,
}

struct ItemArgs {
    #[command(subcommand)]
    pub(super) command: ItemCommand,
}

enum ItemCommand {
    InsertSeparator {
        #[arg(
            long
        )]
        operation_id: Option<String>,
    },
}

struct UpdateArgs {
    #[command(subcommand)]
    pub(super) command: UpdateCommand,
}

enum UpdateCommand {
    Check,
}

struct DiagnosticsArgs {
    #[command(subcommand)]
    pub(super) command: DiagnosticsCommand,
}

enum DiagnosticsCommand {
    Collect,
}

struct ThoughtArgs {
    #[command(subcommand)]
    pub(super) command: ThoughtCommand,
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
            "proqi items insert-separator <session> [--operation-id OP_ID]\n"
            "proqi update check\nproqi diagnostics collect\nproqi thoughts list\n",
        )

        self.assertEqual(errors, [])
        self.assertEqual((actions, surfaces), (2, 7))

    def test_missing_command_and_cli_surface_are_rejected(self) -> None:
        errors, _, _ = check.coverage_errors(
            ACTION_SOURCE,
            "2 actions\n**Create thought**\n",
            CLI_SOURCE,
            "proqi\nproqi -c\nproqi --continue\nproqi -r\nproqi --resume\n"
            "proqi --json capabilities\nproqi -h\nproqi --help\nproqi -V\n"
            "proqi capabilities\nproqi sessions\nproqi update check\n"
            "proqi --json items insert-separator <session> [--operation-id OP_ID]\n"
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
            "proqi items insert-separator <session> [--operation-id OP_ID]\n"
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

    def test_nested_surfaces_are_derived_only_from_the_root_graph(self) -> None:
        without_items = CLI_SOURCE.replace("    Items(ItemArgs),\n", "")
        self.assertNotIn(
            "items insert-separator", check.public_cli_surfaces(without_items)
        )

        with_widgets = CLI_SOURCE.replace(
            "    Items(ItemArgs),\n", "    Items(ItemArgs),\n    Widgets(WidgetArgs),\n"
        ) + '''
struct WidgetArgs {
    #[command(subcommand)]
    pub(super) command: WidgetCommand,
}

enum WidgetCommand {
    List,
}
'''
        self.assertIn("widgets list", check.public_cli_surfaces(with_widgets))

    def test_parent_surface_and_owned_flag_must_be_explicit(self) -> None:
        complete = (
            "proqi\nproqi -c\nproqi --continue\nproqi -r [SESSION]\n"
            "proqi --resume [SESSION]\nproqi --json capabilities\n"
            "proqi -h\nproqi --help\nproqi -V\nproqi --version\n"
            "proqi capabilities\nproqi sessions list\n"
            "proqi items insert-separator <session>\n"
            "proqi update check\nproqi diagnostics collect\nproqi thoughts list\n"
        )

        errors, _, _ = check.coverage_errors(
            ACTION_SOURCE,
            "2 actions\n**Create thought**\n**Delete selected**\n",
            CLI_SOURCE,
            complete,
        )

        self.assertTrue(any("surface 'sessions'" in error for error in errors))
        self.assertTrue(
            any("--operation-id" in error and "insert-separator" in error for error in errors)
        )


if __name__ == "__main__":
    unittest.main()
