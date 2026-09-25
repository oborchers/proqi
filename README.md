<p align="center">
  <img src="assets/proqi-logo.png" width="172" alt="Proqi logo">
</p>

<h1 align="center">Proqi</h1>

<p align="center">
  <code>/pɹˈə͡ʊki/</code>
</p>

<p align="center">
  <strong>The terminal-native prompt composer for power users running multiple coding agents.</strong><br>
  Serious prompting deserves more than a send box.
</p>

<p align="center">
  <a href="https://github.com/oborchers/proqi/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/oborchers/proqi/ci.yml?branch=main&amp;logo=github&amp;label=CI"></a>
  <a href="https://github.com/oborchers/proqi/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/oborchers/proqi?display_name=tag&amp;sort=semver"></a>
  <a href="https://github.com/oborchers/proqi/blob/main/LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-75d6a0"></a>
  <a href="https://github.com/herdrdev/herdr"><img alt="Works best with Herdr" src="https://img.shields.io/badge/works_best_with-Herdr-70d69b"></a>
  <img alt="Rust 1.88 or newer" src="https://img.shields.io/badge/Rust-1.88%2B-000000?logo=rust">
  <img alt="macOS and Linux" src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-30363d">
</p>

<p align="center">
  <img src="assets/proqi-demo.gif" width="1000" alt="Proqi refining, reordering, recovering, and copying independent prompt thoughts">
</p>

[Documentation](https://oborchers.github.io/proqi/) ·
[Why Proqi](#do-you-hate-this-editor) ·
[Workflow](#one-board-many-prompts) ·
[Install](#install) ·
[Controls](#board-controls) ·
[Screenshots](#screenshot-inbox-on-macos) ·
[Herdr](#native-submission-with-herdr) ·
[Herdr plugin](#as-a-herdr-plugin) ·
[CLI](#json-cli-and-agent-skill) ·
[Privacy](#privacy-durability-and-recovery) ·
[Configuration](#configuration)

**Proqi is a terminal-native power-user prompt composer that keeps every next
instruction, screenshot, and alternative as an independent, resumable thought—
ready to refine, reorder, copy, or send to the right coding agent without
interrupting its current work.**

## Do you hate this editor?

<p align="center">
  <img src="assets/codex-composer.png" width="1000" alt="A standard OpenAI Codex terminal prompt field">
</p>

**Not the agent.**<br>
**The input field.**

Codex, Claude Code, and similar CLI harnesses are excellent. Their input still
belongs to one live stream. Draft the next prompt there and an agent question or
wrong turn forces you to cut it out, clear the field, steer, and paste it back.
Alternatives, later prompts, and screenshots spill into temporary files.

An unsent harness draft is not isolated from other senders either. If another
agent submits through Herdr while text is waiting in that input field, the
harness can concatenate both independent instructions and submit them as one
accidental prompt. What looked like a safe draft becomes part of another
agent's message without a distinct turn boundary.

**Proqi is the solution: an agent-ready prompt editor on steroids, built for
power users.** Capture independently; edit, select, duplicate, reorder, add
persistent visual separators, recover, and discover local skills and commands
later.

On macOS, the same board becomes a Screenshot Inbox. Captures arrive as private,
annotatable thoughts: no dragging across panes and no accidental drop into the
agent.

## One board, many prompts

Every thought stays editable. Keep alternatives and choose the next submission
only when ready.

1. Keep working in your terminal while coding agents continue.
2. Define each next work piece in Proqi—not in a live Codex or Claude input.
3. Capture text and screenshots, then edit, annotate, select, and order them.
4. Copy one thought, a range, or the whole board; or submit the ordered board to
   a verified adjacent agent through Herdr.

Proqi is a prompt composer—not a task manager, Markdown IDE, or agent harness.
Standalone work uses the clipboard. Native submission works **only** through
verified [Herdr](https://github.com/herdrdev/herdr), never arbitrary terminals,
agents, or multiplexers.

## Install

### macOS (recommended)

```shell
brew install oborchers/tap/proqi
```

Scope Homebrew trust to this formula when upgrading:

```shell
brew trust --formula oborchers/tap/proqi
brew upgrade --formula oborchers/tap/proqi
```

### Linux and macOS without Homebrew

Proqi 0.10.0 and newer releases include the standalone installer:

```shell
curl -LsSf https://github.com/oborchers/proqi/releases/latest/download/proqi-installer.sh | sh
```

It selects the native archive, verifies its checksum and version, and installs
Proqi without `sudo`. The default destination is `$HOME/.local/bin`; set
`PROQI_INSTALL_DIR` to another absolute directory below `$HOME`. The installer
does not modify `PATH`. If necessary, it prints the required addition.

To inspect the installer before running it:

```shell
curl -LsSf https://github.com/oborchers/proqi/releases/latest/download/proqi-installer.sh -o proqi-installer.sh
less proqi-installer.sh
sh proqi-installer.sh
```

Or use Rust 1.88+:

```shell
cargo install proqi --locked
```

The [latest release](https://github.com/oborchers/proqi/releases/latest) has the
following checked, attested artifacts:

<!-- release-targets:start -->
| OS | CPU | libc | Archive | Debian |
|---|---|---|---|---|
| macOS | ARM64 | system | `proqi-aarch64-apple-darwin.tar.gz` | `-` |
| macOS | x86-64 | system | `proqi-x86_64-apple-darwin.tar.gz` | `-` |
| Linux | x86-64 | glibc >= 2.35 | `proqi-x86_64-unknown-linux-gnu.tar.gz` | `proqi_amd64.deb` |
| Linux | ARM64 | glibc >= 2.35 | `proqi-aarch64-unknown-linux-gnu.tar.gz` | `proqi_arm64.deb` |
| Linux | x86-64 | musl/static fallback | `proqi-x86_64-unknown-linux-musl.tar.gz` | `-` |
| Linux | ARM64 | musl/static fallback | `proqi-aarch64-unknown-linux-musl.tar.gz` | `-` |
<!-- release-targets:end -->

Linux selection uses runtime CPU and libc evidence, not distribution names.
glibc 2.35 or newer receives the GNU build. musl systems and older glibc
receive the statically linked musl fallback. Ambiguous environments stop with
an explanation. Uninstalling preserves data.

### As a Herdr plugin

<span class="version-scope">Next release</span>

Proqi is an agent-optimized terminal scratchpad for follow-up prompts next to
coding-agent sessions. The Herdr plugin opens it beside the focused pane with
one action:

```shell
herdr plugin install oborchers/proqi
```

The plugin needs the first Proqi release that includes `proqi herdr toggle`,
plus Herdr 0.8.0 or newer on macOS or Linux. Until that release is published, a
fresh install receives the latest published Proqi, and the toggle reports it as
too old instead of running.

Herdr previews the plugin before it runs anything. When `proqi` is already
installed, the plugin uses it and installs nothing, so Homebrew, Cargo, Debian,
and standalone installations keep their own update channel. Otherwise the
install step uses `curl` to fetch the standalone installer from the latest
release and checks it against the SHA-256 record published beside it. Both
files come from the same release, so that check proves integrity, not
authenticity; the trust model equals `curl ... | sh` above. Herdr hides the
output of a successful install: a fresh Proqi lands in `$HOME/.local/bin`,
which the plugin finds even when your shell's `PATH` lacks it. If Proqi comes
from Homebrew or Cargo, start the Herdr server from a shell where
`command -v proqi` works.

Bind the toggle in Herdr's `config.toml`:

```toml
[[keys.command]]
key = "prefix+i"
type = "plugin_action"
command = "proqi.toggle"
description = "toggle Proqi"
```

The toggle opens one Proqi pane to the right of the focused pane, focuses it
when it is already open, and closes it once Proqi confirms its edits are saved
when it is focused. Each tab keeps one Proqi session, whichever pane is
focused. A tab's first session is named after the tab's agent when exactly one
agent there has a Herdr name, else after the tab label, or after the stable tab
identity when that label is only Herdr's position number. Herdr does not
restore plugin panes after a cold server restart; the next toggle reopens the
same session and closes the leftover shell if it is still idle. See the
[Herdr plugin guide](docs/guides/herdr-plugin.md).

## Start and resume

```shell
proqi                         # new board
proqi -c                      # latest inactive board here
proqi -r                      # searchable session browser
proqi -r <id-or-name>         # exact session
proqi sessions                # list and search
```

Run `proqi -c` in the agent's project and its last board returns. No temporary
prompt files or unsaved Sublime scratch document.

Changes autosave; exit prints the resume command. Boards rename, trash, restore,
and run in parallel; one lease prevents concurrent editing.

Thoughts may have a short optional name for organization. The name is separate
from the exact body: copying or submitting a thought never prepends it, and
creating a thought never opens a naming prompt.

A genuinely empty board opens with `+ Start typing`. Type or paste immediately
to create the first thought, or click the insertion row to reveal the ordinary
empty editor first. Nothing is saved until content is produced. Press `Esc` to
use Board controls instead. Returning focus to the pane does not override that
Board choice.

`Primary` means logical `Cmd` (`Super` or `Meta`) on macOS and logical `Ctrl`
elsewhere. Proqi receives modifiers only after the operating system, keyboard
remapper, and terminal have handled the key. Raw `Ctrl` is not a second Primary
modifier on macOS.

Complete user documentation for the published 0.11.0 product, with main-only
next-release additions clearly labeled, starts at the
[Proqi documentation home](https://oborchers.github.io/proqi/). Use the
[complete feature index](https://oborchers.github.io/proqi/reference/features.html)
to discover the full
surface, or begin with a workflow:

- [Capture and edit thoughts](https://oborchers.github.io/proqi/guides/capture-and-edit.html)
- [Select and act on thoughts](https://oborchers.github.io/proqi/guides/selection.html)
- [Transform and organize thoughts](https://oborchers.github.io/proqi/guides/edit-and-transform.html)
- [Configure and troubleshoot shortcuts](https://oborchers.github.io/proqi/guides/shortcuts.html)
- [Paste, clean up, and attach files](https://oborchers.github.io/proqi/guides/paste-and-attachments.html)
- [Discover commands, skills, and collaborators](https://oborchers.github.io/proqi/guides/discovery-and-invocations.html)
- [Deliver prompts to agents](https://oborchers.github.io/proqi/guides/agent-delivery.html)
- [Organize sessions and recover work](https://oborchers.github.io/proqi/guides/organization-and-recovery.html)
- [Browse every Commands action](https://oborchers.github.io/proqi/reference/commands.html)
- [Use the complete CLI](https://oborchers.github.io/proqi/reference/cli.html)

The Markdown source remains available from
[`docs/index.md`](docs/index.md) for offline reading and contribution.

The tables below describe the factory map. The complete stable action and
context inventory is in [context/KEYMAP_ACTIONS.md](context/KEYMAP_ACTIONS.md).
Help and footer labels always show the bindings resolved from the active
configuration.

### Board controls

| Input | Action |
| --- | --- |
| `n`, `Enter` on `+ New thought`, paste, or click | Create a thought |
| Commands: `Insert separator` | Insert a persistent visual separator below the focused item |
| `Primary+V` / `p` with no selection | Paste exactly as a new thought |
| `j` / `k` or arrows | Focus next / previous; twice at a blocked bottom / top edge creates there |
| `Ctrl+↓` / `↑` or `Ctrl+j` / `k` | Focus the last / first live thought without wrapping |
| macOS `Ctrl+N` / `Ctrl+Shift+N`; elsewhere `Alt+↓` / `↑` or `Alt+j` / `k` | Insert a blank below / above the focused thought and edit it |
| `Page Up` / `Page Down` | Move five thoughts previous / next |
| `Enter` or `e` | Edit |
| `Ctrl+R` | Edit or clear the focused thought's optional name |
| macOS `Option+Shift+↓` / `↑`; `Primary+J` / `Primary+K`, `Primary+Shift+↓` / `↑`, or drag | Reorder |
| `Primary+C` / `y`; `Primary+X` / `x` | Copy; safe cut |
| `d` or `Del` (`Entf` on German keyboards) | Delete |
| `Space`; `Primary+A` / `a` | Toggle selection; select all |
| `Shift+↑` / `↓`, `K` / `J`, or `Shift+Page Up` / `Shift+Page Down`; `v` then move | Extend by one, extend by five, or latch a range |
| macOS `Ctrl+Shift+↓` / `↑` or `Ctrl+Shift+J` / `K` | Extend the anchored range to the last / first live thought |
| `Primary+D` / `Shift+D` | Duplicate thought or selection |
| `Primary+Enter` / `s`; `Primary+Shift+Enter` / `Shift+S`; then arrows or `h` / `j` / `k` / `l` if needed | Submit and remove after acceptance; submit and keep |
| macOS `Ctrl+Z`; `Primary+Z` / `u` | Undo a board operation |
| macOS `Ctrl+Shift+Z` / `Ctrl+Y`; `Primary+Shift+Z` / `Primary+Y` | **Redo a board operation** |
| `Primary+Shift+V` / `Shift+P` | Paste and clean up spacing |
| `f` | Clean up spacing in selected Board thoughts or the focused thought |
| `c`; `/`; `:`; `i`; `?` | Collapse; search; commands; Screenshot Inbox; help |
| `Esc`; `Primary+Q` / `q` | Clear selection; exit after durable flush |

### Editor controls

| Input | Action |
| --- | --- |
| `Esc` | Return to the board |
| `Ctrl+R` | Edit or clear this thought's optional name without changing the body selection |
| `Primary+A`; `Primary+U` | Select all; delete logical line |
| `Primary+Shift+U` | Delete containing sentence |
| macOS `Ctrl+Z`; `Ctrl+Shift+Z` / `Ctrl+Y`; retained Primary aliases elsewhere | Undo; redo |
| `Primary+C` / `X`; `Primary+V` | Native copy / safe cut; paste exactly |
| `Primary+Shift+V` | Paste and clean up spacing |
| `Ctrl+Shift+F` | Clean up spacing in the complete active thought |
| macOS: `Cmd+←` / `→` | Move to the current wrapped visual-row start / end |
| macOS: `Option+←` / `→`; elsewhere: `Ctrl+←` / `→` | Move by word |
| macOS: `Ctrl+←` / `→`; elsewhere: `Alt+←` / `→`; `Home` / `End` | Move to the logical line start / end |
| `Shift` + movement | Extend text selection |
| macOS: `Cmd+Shift+←` / `→` | Extend to the current wrapped visual-row start / end |
| `Alt+↑` / `↓` or `Page Up` / `Page Down`; `Ctrl+↑` / `↓` | Jump five rows; complete thought start / end |
| `Enter`; `Tab`; `Shift+Tab` | Continue lists; nest a recognized list or insert spaces; outdent a recognized list while leaving ordinary text unchanged |
| `↑` / `↓` twice at a boundary | Focus the adjacent thought, or create at the top / bottom board edge |
| `Primary+Enter`; `Primary+Shift+Enter` | Submit and remove after acceptance; submit and keep |
| Type `$name`, `/name`, or supported `@name` | Fuzzy-find and complete a local invocation |
| `↑` / `↓` or `Primary+P` / `Primary+N`; `Enter` / `Tab`; `Esc` | Navigate, insert, or close invocation results |

### Overlay and input controls

| Active owner | Factory controls |
| --- | --- |
| Help, update, Screenshot Inbox, release highlights | `↑` / `↓` or `k` / `j`; `Page Up` / `Page Down`; `Enter` when a choice is offered; `Esc` |
| Commands, search, transfer, global-delivery query | Type to filter; `↑` / `↓`; `Alt+↑` / `↓` or `Page Up` / `Page Down`; `Enter`; `Backspace` / `Delete`; text cursor keys; `Esc` |
| Invocation and invocation query | Type to filter; `↑` / `↓` or `Primary+P` / `Primary+N`; `Alt+↑` / `↓` or page keys; `Enter` / `Tab`; `Esc` |
| Direction chooser | Arrows or `h` / `j` / `k` / `l`; `Enter`; `Esc` |
| Global-delivery disposition | `↑` / `↓` or `k` / `j`; page keys; `Enter`; `Esc` |
| Session Browser and Browser query | Type to filter; `↑` / `↓`; `Home` / `End`; `Alt+↑` / `↓` or page keys; `Enter`; `Backspace` / `Delete`; `F2` rename and `F8` trash while the query is empty; `Esc` |
| Thought name, Rename, and Browser rename | Type and use text cursor, `Backspace`, or `Delete`; `Enter` confirms; `Esc` cancels |
| Recovery | `r` retry storage; `w` export recovery; `q` or `Primary+Q` exits through durability handling; `Esc` remains the invariant close route |
| Empty insertion boundary | Board controls remain available; `Enter` or `n` creates; range and reorder actions are thought-only no-ops; `Esc` returns to the final thought |

By default, unmodified `Del` and `d` share the Board delete action. The versioned
keymap can replace or disable either alias. Modified `Del` is unbound on the
Board by default. Text contexts reserve ordinary, shifted, Option/Alt and
AltGr-compatible printable input. Named editing keys remain contextual.
The session Browser uses F2 to rename and F8 to trash while its query is empty;
uppercase R and D remain search text. List and direction defaults preserve
symmetric arrow and Vim-style navigation.

Primary chords and Board characters such as `y`, `x`, `u`, `s`, `Shift+S`, and `q`
are ordinary aliases of the same configurable actions. macOS additionally uses
raw `Ctrl+Z`, `Ctrl+Shift+Z`, and `Ctrl+Y` as terminal-safe history aliases. Raw
Control remains distinct from Primary for every unrelated action. A host can
consume a chord before Proqi receives it. A host-performed bracketed paste stays
exact.

### Ghostty shortcut delivery

Ghostty resolves its own keybindings before bytes enter the terminal PTY.
Herdr and Proqi therefore receive nothing when a Ghostty action consumes a
chord, and they receive only replacement bytes when Ghostty rewrites one. Check
the defaults of the installed Ghostty version with:

```sh
ghostty +list-keybinds --default
```

On macOS, Ghostty currently uses `Cmd+Enter` for fullscreen and
`Cmd+Shift+Enter` for split zoom. It also rewrites `Cmd+Left` and `Cmd+Right`
to raw `Ctrl+A` and `Ctrl+E`. Add only the overrides whose chords Proqi should
receive:

```ini
# Let Proqi receive its Primary submission aliases.
keybind = super+enter=unbind
keybind = super+shift+enter=unbind

# Forward modified horizontal arrows instead of Ctrl+A and Ctrl+E.
keybind = super+arrow_left=unbind
keybind = super+arrow_right=unbind

# Emit logical Super+Shift+v for Paste and clean up spacing.
keybind = super+shift+v=csi:118;10u
```

The arrow overrides have also been verified through a real remapped Ghostty,
Herdr, and Proqi chain. Reload the configuration with `Cmd+Shift+,` or restart
Ghostty. A remapper may change which physical key produces logical `Cmd`, so
inspect the event Proqi actually receives rather than relying on a keycap.

The CSI-u example passes Ghostty's config validator and its bytes are covered
by real macOS PTY tests. These examples are not guarantees for every keyboard
layout, Ghostty version, or host mapping. Proqi never modifies host
configuration. Inspect delivery in the relevant context with:

```sh
proqi diagnostics keypress --context board,edit --timeout-ms 5000
proqi --json diagnostics keypress --context board --defaults
```

Capture reports the logical key, exact modifiers, phase, state, selected context
and configured action. Escape cancels. `--defaults` works even with invalid
configuration. A timeout reports no key event received; Proqi cannot know which
layer, if any, consumed the chord. It records no paste, session content or raw
terminal responses. On macOS, use the terminal-safe Control history aliases,
the Board `u` fallback, or Commands when a Primary history chord is blocked.

Other macOS defaults assign application behavior to `Cmd+Q`, `Cmd+A`, `Cmd+D`,
`Cmd+J`, `Cmd+K`, Command plus vertical arrows, and clipboard or history
chords. `performable:` passes through only when its Ghostty action is unavailable
and is not a general TUI fallthrough. See
[Ghostty keybindings](https://ghostty.org/docs/config/keybind).

If a keyboard remapper maps `Home` and `End` to `Cmd+Left` and `Cmd+Right`, both
physical routes have the same downstream identity. Proqi cannot reconstruct
their origin. Logical-line movement therefore prefers `Ctrl+Left` and
`Ctrl+Right` on macOS, and `Alt+Left` and `Alt+Right` elsewhere. Named `Home`
and `End` remain compatible aliases when those events actually arrive.

Exact paste is always the default. Explicit spacing cleanup preserves authored
line breaks, collapses repeated spaces and tabs, and reduces multiple blank lines
to one paragraph break. It preserves recognized list structure and leaves code,
tables, quotes, paths, URLs, controls, and annotated semantic ranges unchanged.
Large-paste folds are recomputed from the transformed content.

Mouse input covers the same core workflow. Images, files, and large pastes fold
into compact annotations while their content stays intact. In Edit mode, an
unmodified `Space` on one completely selected collapsed annotation inserts a
space immediately before it without replacing it. See
[invocation compatibility](docs/INVOCATIONS.md).

Invocation lookup accepts compact ordered abbreviations such as `$aos-ce` for
`$aos-communication-email`. Exact and prefix matches remain strongest, followed
by contiguous and separator-aware fuzzy matches. The typed sigil remains a hard
namespace boundary, so slash, dollar, and at forms never mix.

Inside Herdr, opening the same invocation picker also discovers recognized live
coding agents across the server. Selecting one inserts an inert collaborator
location and displays it as a compact inline mention. It never focuses or
submits to that agent.

## Screenshot Inbox on macOS

<p align="center">
  <img src="assets/proqi-screenshot-inbox.gif" width="1000" alt="Proqi enabling Screenshot Inbox, receiving a new macOS screenshot, and turning it into an annotatable thought">
</p>

From `+ Start typing`, press `Esc`, then `i`: new Desktop screenshots become
annotatable image thoughts. From an ordinary Board, press `i` directly. Proqi
never takes, uploads, analyzes, copies, or configures them.

One process listens. It pauses after 10 unattended captures or 20 inactive
minutes. Resume ignores the gap; failed imports require explicit retry.

macOS may request terminal **Files & Folders** access—not Screen Recording or
Accessibility. Linux reports macOS-only availability.

```toml
[screenshot_inbox]
# directory = "/absolute/path/to/an/isolated/inbox" # default: Desktop
capture_all_new_images = false
notify_terminal_on_auto_pause = false
```

## Native submission with Herdr

<p align="center">
  <img src="assets/proqi-herdr-workflow.png" width="1000" alt="A Herdr workspace with Codex working beside a Proqi board of prepared prompt thoughts">
</p>

On macOS, `Ctrl+Enter` and `Ctrl+Shift+Enter` are the terminal-safe submission
defaults. They address the same actions as the retained `Primary+Enter` and
`Primary+Shift+Enter` aliases. Plain Enter remains newline and smart-list
continuation in the editor. These are logical events received from the terminal,
not claims about physical modifier keys. Linux and Windows defaults are unchanged.

In Herdr, Proqi finds verified adjacent agents. In Board mode, `s` or
`Primary+Enter` submits the selected thought or selection in visible order and
removes after acceptance; `Shift+S` or `Primary+Shift+Enter` keeps it. While editing,
the same Primary chords submit only the active thought. The palette submits the
whole board. With several verified adjacent agents, either edit
chord opens the temporary direction chooser; press an arrow or `h`, `j`, `k`,
or `l` next to choose the target. Those keys select a direction instead of
moving or inserting text while the chooser is open. `Esc` cancels the chooser
and returns to the unchanged editor.

When an accepted submission removes the final thought, Proqi returns to the
passive `+ Start typing` board. It does not create a replacement blank thought;
the next typed or pasted content creates the next thought directly.

Busy receivers decide whether input steers or queues. Any failed verification
leaves the board unchanged.

Keep deferred prompts in Proqi rather than in the native harness input when
other senders can target that agent. When two submissions overlap, the harness
may not keep them in separate turns, so text already waiting in its input can
merge with an incoming message. Proqi preserves its verified submission flow,
but it cannot separate content after the receiving harness has combined it.

Proqi never invokes a shell, injects keys, reads chats, or waits. Herdr is
optional.

Protocol 19 supports Claude Code, Codex, Pi, and Hermes.
[OpenCode](context/harnesses/opencode.md) and [Kilo](context/harnesses/kilo.md)
are conditional; Cline is deferred.

## JSON CLI and agent skill

The CLI also exposes versioned JSON:

```shell
proqi --json capabilities
printf '%s' 'Review this.' | proqi --json thoughts add <session-id>
proqi --json thoughts rename <session-id> <thought-id> 'Release plan'
proqi --json thoughts send <source> <thought-id> <destination> --remove
```

Thought list and inspect JSON include nullable `name` metadata. Cross-session
send preserves it, while human inspect and agent submission remain body-only.
Thought listings retain their content-bearing `thoughts` projection and also
return an ordered typed `items` projection. A separator is reported as
`kind: "separator"` with its own `sep_` identity and never as an empty thought.

The [Proqi skill](skills/proqi/SKILL.md) uses it without scraping the TUI:

```shell
npx skills add oborchers/proqi --skill proqi -g --agent codex --agent claude-code
```

The skill does not install the Proqi executable. Run `capabilities` first.

For read-only-first failure investigation:

```shell
npx skills add oborchers/proqi --skill proqi-debug -g
```

### Claude Code plugin

Claude Code can install both skills from this repository's plugin marketplace
instead:

```text
/plugin marketplace add oborchers/proqi
/plugin install proqi@proqi
```

The plugin ships the same `skills/proqi` and `skills/proqi-debug` files, invoked
as `/proqi:proqi` and `/proqi:proqi-debug`. It follows the default branch and
reports a new version when a release changes the Cargo version. Update with
`/plugin marketplace update proqi`, then `/plugin update proqi@proqi`. It does
not install the Proqi executable.

## Privacy, durability, and recovery

Thoughts, attachments, settings, and redacted logs stay local. No telemetry,
cloud sync, collaboration service, or upload.

The footer reports durability. Failures block destructive exit and remain
retryable/exportable. Editor and board history survive restart.

If the terminal watchdog confirms that only the Crossterm input lane has
stopped making progress while the pane is still usable, Proqi first drains
accepted work and restores the terminal. On macOS and Linux it then makes one
same-pane replacement attempt for the exact session. Board, Compose, and Edit
state resume in the inherited PTY. A replacement that cannot prove fresh input
progress exits with an exact manual resume command. The per-session circuit
allows at most two automatic recoveries in any rolling ten-minute window.
If persistence has already failed, Proqi does not replace the process. It first
writes the existing private recovery export, with a bounded fallback under the
private runtime root if the primary recovery directory is unavailable. It then
exits with both the exact resume command and the optimistic-state recovery path.
Recovery format 2 retains thoughts and payload-free separators, including their
exact identities, shared ordering, timestamps, and recoverable deletion state.

```shell
proqi doctor
proqi diagnostics collect --output proqi-diagnostics.json
```

Diagnostics are redacted and local; review before sharing. See
[SECURITY.md](SECURITY.md). Disable content-free update checks with
`check_for_updates = false`.
Collected update diagnostics include only closed lifecycle stages, aggregate
participant and replacement counts, stable failure codes, and convergence.
Finalization diagnostics distinguish unavailable control, unavailable private
cache state, and an exact-state mismatch without recording local identifiers.
Input recovery diagnostics contain only a stable stage, reason, attempt count,
and outcome. They never contain session identity, paths, pane identity, terminal
bytes, or thought content.

## Configuration

Optional platform-native `config.toml`:

```toml
check_for_updates = true
theme = "auto" # auto, light, dark, limited, or a bounded local theme file
density = "comfortable" # or compact
merge_separator = "\n\n" # one blank line between merged thoughts
mouse_capture = true # set false if your terminal/multiplexer mishandles mouse reporting
footer_hidden = false # set true to reclaim optional persistent footer chrome

[keymap]
schema_version = 1

[keymap.bindings.board]
"submission.submit_remove" = [
  { key = "Enter", modifiers = ["Primary"] },
  { key = "s" },
  { key = "F5" },
]
"submission.submit_keep" = [] # keyboard aliases disabled; Commands stays available
"thought.delete" = [{ key = "d" }, { key = "Delete" }]
"thought.rename" = [{ key = "r", modifiers = ["Control"] }]

[keymap.macos.edit]
"submission.submit_remove" = [{ key = "Enter", modifiers = ["Super", "Alt"] }]

[keymap.portable.edit]
"submission.submit_remove" = [{ key = "F5" }]
```

`footer_hidden` sets the startup state. In Board, the unmodified `h` key (the remappable
`footer.toggle` action) changes visibility only for the current Proqi process;
restart restores the configured state.
Because configuration rejects unknown fields, remove `footer_hidden` before
running an older Proqi release.

Each supplied context/action list replaces all its default aliases. Omitted
pairs retain defaults; platform overrides replace common lists. Control, Alt,
Shift, Super, Meta and Hyper are independent logical modifiers. Primary expands
to Super/Meta on macOS and Control elsewhere. Unknown identifiers, collisions,
text theft, and removal of Escape or required recovery routes fail before
terminal setup.

The legacy `[keybindings]` table remains accepted through explicit translation,
including its historical aliases. It cannot be mixed with `[keymap]`. See the
[versioned contract and migration guide](context/SHORTCUTS.md) and
[complete action/context inventory](context/KEYMAP_ACTIONS.md).
Thought transformations retain their default Primary+T and Board `t` behavior;
Commands remains available for split, extract and merge when a chord is unbound.

Unsafe theme contrast is rejected. See the
[theme example](docs/themes/proqi-dark.toml). Invocation roots stay local.
Sentence deletion uses a documented Unicode profile with unavoidable ambiguity.
See [sentence deletion](docs/SENTENCE_DELETION.md).
Visual-row selection uses the current rendered width and folded presentation.
On macOS, Cmd plus horizontal arrows uses the current wrapped row and
Option retains word movement. Elsewhere, Ctrl plus horizontal arrows retains
word movement, including with Shift. If the terminal intercepts the macOS
Cmd-arrow selection chords, use the command palette or the configured
versioned `editor.extend_visual_row_start` and `editor.extend_visual_row_end` aliases.

## Compatibility and contributing

Proqi supports the macOS and Linux targets listed under Install; only the latest
`0.x` is supported.
It is an MIT-licensed binary. Contributors: [CONTRIBUTING.md](CONTRIBUTING.md),
[PRODUCT.md](context/PRODUCT.md), [ARCHITECTURE.md](context/ARCHITECTURE.md).

```shell
cargo build --locked
cargo run --bin proqi
cargo xtask check      # iterative local gate
cargo xtask check-full # canonical final gate
```

The demos use the release binary; the assets gate checks dimensions, links, and
privacy.
