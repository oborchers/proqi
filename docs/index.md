# Keep the next prompt out of the live input

<span class="version-scope">Documentation for Proqi 0.11.0</span>

Proqi is a terminal-native prompt composer for people coordinating several
coding agents. Capture each next instruction as an independent thought, refine
it without interrupting an agent, then copy or deliver it when the time is
right.

No account, cloud service, or save command is involved. Proqi stores work
locally, reports whether each change is durable, and keeps board and editor
history across restarts.

<div class="feature-paths" markdown>

<div markdown>
## Start a board

Install Proqi, understand Board and Edit, and learn how automatic saving works.

[Get started](getting-started.md)
</div>

<div markdown>
## Compose precisely

Paste exact text, clean wrapped prose, attach files, edit Unicode, and transform
thoughts.

[Capture and edit](guides/capture-and-edit.md)
</div>

<div markdown>
## Coordinate agents

Copy anywhere or deliver through a verified Herdr target with explicit keep and
remove semantics.

[Deliver prompts](guides/agent-delivery.md)
</div>

<div markdown>
## Discover everything

Browse every shipped capability, all 59 Commands actions, and the complete CLI.

[Open the feature index](reference/features.md)
</div>

</div>

## What makes Proqi different

- **Thoughts are independent.** Drafting the next request never occupies an
  agent's live composer.
- **Content stays exact.** Names and separators organize the board without
  entering copied or delivered prompt bodies.
- **Selection is deliberate.** Focus, arbitrary selection, contiguous ranges,
  and text selection have distinct visible meanings.
- **Delivery is truthful.** Local removal happens only after a matching accepted
  receipt. Copying is never described as delivery.
- **Recovery is part of the workflow.** Durability state, persistent history,
  session trash, retry, and private recovery export are visible rather than
  hidden.

## Find an action while working

Press ++colon++ in Board mode to open **Commands**. Search by what you want to
do and press ++enter++, or click the result. Commands only offers actions that
are valid for the current state.

Press ++question++ for contextual Help. Help and the footer show effective
configured bindings, not a stale factory table.

## Choose a path

| Goal | Start here |
| --- | --- |
| Install and open the first board | [Get started](getting-started.md) |
| Learn every feature Proqi ships | [Complete feature index](reference/features.md) |
| Look up a Commands action | [Commands palette reference](reference/commands.md) |
| Work with the command line or JSON | [CLI reference](reference/cli.md) |
| Recover work safely | [Sessions and recovery](guides/organization-and-recovery.md) |
| Fix a shortcut that never arrives | [Shortcut troubleshooting](guides/shortcuts.md) |

Proqi is a prompt composer, not a task manager, Markdown IDE, or agent harness.
Standalone use relies on the clipboard. Direct agent delivery is available only
through verified [Herdr](https://github.com/herdrdev/herdr) integration.
