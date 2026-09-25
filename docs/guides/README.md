# Proqi user guides

> Applies to Proqi 0.11.0. These guides describe behavior shipped in that
> release. The transformation guide also labels the additive CLI operations
> already on `main` for the next release.

Proqi is a local, resumable prompt board. Each thought stays editable. Copying
never changes it, and **Submit and keep** retains it. For direct agent delivery,
an accepted **Submit** removes the local source, and that removal is undoable.
Start with the workflow you need:

| I want to... | Guide |
| --- | --- |
| Create and edit exact prompt text | [Capture and edit thoughts](capture-and-edit.md) |
| Work with one thought or a group | [Select and act on thoughts](selection.md) |
| Split, extract, merge, reorder, or separate thoughts | [Transform and organize thoughts](edit-and-transform.md) |
| Make shortcuts reach Proqi reliably | [Configure and troubleshoot shortcuts](shortcuts.md) |
| Paste readable text or attach local files | [Paste, clean up, and attach files](paste-and-attachments.md) |
| Turn new macOS screenshots into thoughts | [Use Screenshot Inbox](paste-and-attachments.md#screenshot-inbox-on-macos) |
| Find Proqi actions, skills, commands, or collaborators | [Discover commands, skills, and collaborators](discovery-and-invocations.md) |
| Send a prompt to a verified coding agent | [Deliver prompts to agents](agent-delivery.md) |
| Open Proqi beside a Herdr agent with one key | [Use Proqi as a Herdr plugin](herdr-plugin.md) |
| Name, separate, resume, or recover work | [Organize sessions and recover work](organization-and-recovery.md) |
| Check for updates or restart safely | [Update Proqi and active sessions](updates.md) |
| Change themes, list behavior, density, or mouse capture | [Configure appearance and behavior](configuration.md) |

## The three things to know first

1. **Focus and selection are different.** Focus identifies the item that
   navigation and single-item actions address. Selection builds a group for
   bulk actions. See [Select and act on thoughts](selection.md).
2. **Copying and delivery are different.** Unannotated copying uses the native
   clipboard when available and a bounded terminal fallback where supported;
   annotated copy and cut require macOS typed clipboard support. Direct
   delivery is available only through a verified Herdr integration. See
   [Deliver prompts to agents](agent-delivery.md).
3. **There is no save command.** Proqi saves accepted changes automatically and
   shows durability in the footer. A storage failure keeps the in-memory work
   visible and offers explicit retry or recovery export choices. See
   [Organize sessions and recover work](organization-and-recovery.md#recover-from-a-storage-failure).

## Find actions without memorizing keys

Press `:` in Board mode to open **Commands**. Search for an action by name, then
press `Enter` or click it. Commands uses the same availability rules as the
keyboard and mouse controls, so an unavailable action is not presented as if it
could run.

Press `?` for contextual Help. Help and the footer show the effective bindings
after configuration, not merely the factory defaults.

For exhaustive discovery, use the [complete feature index](../reference/features.md),
the [Commands reference](../reference/commands.md), and the
[CLI reference](../reference/cli.md). For local history behavior, see
[Undo and redo](../reference/undo-redo.md).

## Versioning

The human CLI, configuration, and machine JSON may change between Proqi minor
releases before 1.0. Use the guides that match the installed version shown by:

```sh
proqi --version
```

Release-specific changes belong in [GitHub Releases](https://github.com/oborchers/proqi/releases).
These guides intentionally cover user workflows, not internal architecture or
handoff contracts.
