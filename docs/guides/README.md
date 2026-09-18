# Proqi user guides

> Applies to Proqi 0.11.0. These guides describe behavior shipped in that
> release.

Proqi is a local, resumable prompt board. Each thought stays editable until you
copy it or deliberately deliver it to an agent. Start with the workflow you
need:

| I want to... | Guide |
| --- | --- |
| Work with one thought or a group | [Select and act on thoughts](selection.md) |
| Make shortcuts reach Proqi reliably | [Configure and troubleshoot shortcuts](shortcuts.md) |
| Paste readable text or attach local files | [Paste, clean up, and attach files](paste-and-attachments.md) |
| Turn new macOS screenshots into thoughts | [Use Screenshot Inbox](paste-and-attachments.md#screenshot-inbox-on-macos) |
| Send a prompt to a verified coding agent | [Deliver prompts to agents](agent-delivery.md) |
| Name, separate, resume, or recover work | [Organize sessions and recover work](organization-and-recovery.md) |

## The three things to know first

1. **Focus and selection are different.** Focus identifies the item that
   navigation and single-item actions address. Selection builds a group for
   bulk actions. See [Select and act on thoughts](selection.md).
2. **Copying and delivery are different.** Copying writes exact thought bodies
   to the native clipboard. Direct delivery is available only through a
   verified Herdr integration. See [Deliver prompts to agents](agent-delivery.md).
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

For the complete stable action identifiers and contexts, use the
[keymap action inventory](../../context/KEYMAP_ACTIONS.md). For the full undo
model, use the canonical [undo and redo guide](../UNDO_REDO.md).

## Versioning

The human CLI, configuration, and machine JSON may change between Proqi minor
releases before 1.0. Use the guides that match the installed version shown by:

```sh
proqi --version
```

Release-specific changes belong in [GitHub Releases](https://github.com/oborchers/proqi/releases).
These guides intentionally cover user workflows, not internal architecture or
handoff contracts.
