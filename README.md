<p align="center">
  <img src="assets/proqi-logo.png" width="172" alt="Proqi logo">
</p>

<h1 align="center">Proqi</h1>

<p align="center">
  <code>/pɹˈə͡ʊki/</code>
</p>

<p align="center">
  <strong>The agent-ready prompt editor for terminal power users.</strong><br>
  Stop drafting serious prompts in a send box.
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
  <img src="assets/proqi-demo.gif" width="1000" alt="Proqi refining independent prompt thoughts, reordering them, and copying an ordered selection">
</p>

[Why Proqi](#do-you-hate-this-editor) ·
[Workflow](#one-board-one-exact-prompt) ·
[Install](#install) ·
[Controls](#board-controls) ·
[Screenshots](#screenshot-inbox-on-macos) ·
[Herdr](#native-submission-with-herdr) ·
[CLI](#json-cli-and-agent-skill) ·
[Privacy](#privacy-durability-and-recovery) ·
[Configuration](#configuration)

**Proqi is a terminal-native power-user prompt composer that turns scattered
thoughts, screenshots, and local context into exact, recoverable prompts for
multiple coding agents—without interrupting their work.**

## Do you hate this editor?

<p align="center">
  <img src="assets/codex-composer.png" width="752" alt="The standard OpenAI Codex terminal prompt field">
</p>

Not the agent. The input field.

Codex, Claude Code, and similar CLI harnesses are excellent conversational
interfaces. But their prompt surface belongs to one active stream. Drafting a
later prompt, preserving alternatives, combining several thoughts, collecting
screenshots, or steering another agent interrupts that flow—or sends you to a
temporary document and back.

**Proqi is the solution: a prompt editor on steroids for people running more
than one coding agent.** Capture rough thoughts independently. Edit exact
Unicode text. Undo or redo editing and board operations after restart. Select
ranges, reorder them, or concatenate the entire board into one deliberate
prompt. Discover local skills, commands, and agents while writing, without
executing them.

On macOS, one board can also become a Screenshot Inbox. New captures arrive as
private, annotatable thoughts—no dragging across panes, no accidental drop into
an agent.

Agent-ready. Local-first. Built for power users.

## One board, one exact prompt

1. Keep working in your browser or editor while the coding agent continues.
2. Capture rough text and screenshots into Proqi as independent thoughts.
3. Refine, annotate, select, and order the parts of the next prompt.
4. Copy the exact ordered board anywhere, or submit it to a verified adjacent
   agent through Herdr—without first interrupting that agent.

| Power-user friction | Proqi |
| --- | --- |
| Half-formed ideas collide in one prompt field | Durable independent thoughts |
| Several sources must become one prompt | Range selection, ordering, and whole-board assembly |
| Long context becomes hard to edit | Exact multiline editing, folding, lists, and fast navigation |
| A revision or deletion goes wrong | Persistent editor and board undo/redo |
| Local agent tooling is hard to remember | Skill, command, and agent discovery while authoring |
| Screenshots land in the wrong pane | One-board macOS Screenshot Inbox |
| An agent is still working | Compose separately; copy or submit when ready |

Proqi is a prompt composer, not a task manager, Markdown IDE, or agent harness.
Its standalone workflow uses the native clipboard and works without an account
or integration. Native adjacent-agent submission is supported **only** through
verified [Herdr](https://github.com/herdrdev/herdr) integration—not arbitrary
terminals, agents, or multiplexers.

## Install

Homebrew is recommended on macOS and supported Linux systems:

```shell
brew install oborchers/tap/proqi
```

Homebrew may ask you to trust the personal tap. Trust only this formula when
whole-tap trust is unnecessary:

```shell
brew trust --formula oborchers/tap/proqi
brew upgrade --formula oborchers/tap/proqi
```

Or build the binary with Rust 1.88 or newer:

```shell
cargo install proqi --locked
```

Checksummed standalone archives cover Apple silicon and Intel macOS, plus
x86-64 GNU/Linux with glibc 2.35 or newer. Debian and Ubuntu users can install
the release `proqi_amd64.deb`; there is no APT repository. Download only from
the [latest GitHub Release](https://github.com/oborchers/proqi/releases/latest),
verify the adjacent SHA-256 file, and keep `proqi-installation.json` beside a
manually installed archive binary. Archives include signed GitHub provenance,
SBOMs, notices, and shell completions.

Proqi never runs `sudo`, package managers, or self-update commands without an
explicit user action. Uninstalling the binary deliberately preserves local
sessions and configuration.

## Start and resume

```shell
proqi                         # new board
proqi -c                      # latest inactive board here
proqi -r                      # searchable session browser
proqi -r <id-or-name>         # exact session
proqi sessions                # list and search
```

Paste or press `n` to start. Changes autosave, and exit prints the exact resume
command. Sessions can be renamed, trashed, restored, and safely used in
parallel; one exclusive lease prevents two processes editing the same board.

`Primary` means `Command` on macOS and `Ctrl` on Linux.

### Board controls

| Input | Action |
| --- | --- |
| `n`, `Enter` on `+ New thought`, paste, or click | Create a durable thought |
| `Primary+V` with no selection | Create from the native clipboard |
| `j` / `k` or arrows | Focus next / previous |
| `Enter` or `e` | Edit |
| `Primary+J` / `Primary+K`, `Primary+Shift+↓` / `↑`, or drag | Reorder |
| `y` / `Primary+C`; `x` / `Primary+X`; `d` | Copy; safe cut; delete |
| `Space`; `a` / `Primary+A` | Toggle selection; select all |
| `Shift+↑` / `↓` or `K` / `J`; `v` then move | Extend range; latch range mode |
| `Primary+D` | Duplicate thought or selection |
| `s`; `S`; then arrows or `h` / `j` / `k` / `l` if needed | Submit and remove after acceptance; submit and keep |
| `u` / `Primary+Z` | Undo a board operation |
| `Primary+Shift+Z` / `Primary+Y` | **Redo a board operation** |
| `c`; `/`; `:`; `i`; `?` | Collapse; search; commands; Screenshot Inbox; help |
| `Esc`; `q` / `Primary+Q` | Clear selection; exit after durable flush |

### Editor controls

| Input | Action |
| --- | --- |
| `Esc` | Return to the board |
| `Primary+A`; `Primary+U` | Select all; delete logical line |
| `Primary+Z`; `Primary+Shift+Z` / `Primary+Y` | Undo; redo |
| `Primary+C` / `X` / `V` | Native copy / safe cut / paste |
| `Alt` or `Ctrl` + `←` / `→`; `Home` / `End` | Move by Unicode word; line boundary |
| `Shift` + movement | Extend text selection |
| `Alt+↑` / `↓`; `Primary+↑` / `↓` | Jump five visual rows; thought start / end |
| `Enter`; `Tab`; `Shift+Tab` | Continue lists; nest; outdent |
| `↑` / `↓` twice at a boundary | Focus the adjacent thought |
| Type `$name`, `/name`, or supported `@name` | Complete a discovered local invocation |
| `↑` / `↓` or `Primary+P` / `Primary+N`; `Enter` / `Tab`; `Esc` | Navigate; insert; close invocation results |

Mouse input covers creation, focus, cursor placement, text and board ranges,
folds, scrolling, reordering, controls, and verified Herdr targets. Images,
files, and large pastes render as compact annotations while their exact content
remains intact. See [invocation compatibility](docs/INVOCATIONS.md).

## Screenshot Inbox on macOS

Press `i` or choose **Enable Screenshot Inbox**. Proqi watches the current
user's Desktop by default and turns only new, completed macOS screenshots into
annotatable image thoughts. It never takes screenshots, changes macOS settings,
uploads or analyzes images, or copies the source file. macOS may request **Files
& Folders** access for the named terminal host; Screen Recording and
Accessibility are not used.

Only one Proqi process listens at a time. A verified contender can ask the
current owner to drain accepted captures before takeover; a live owner is never
force-unlocked. Listening pauses after 20 inactive minutes or 10 unattended
captures. **Resume Screenshot Inbox** starts from a fresh snapshot, so captures
made while paused are not imported later.

```toml
[screenshot_inbox]
# directory = "/absolute/path/to/an/isolated/inbox" # default: macOS Desktop
filename_patterns = []
capture_all_new_images = false
supported_types = ["png", "jpeg", "tiff"]
min_file_bytes = 64
max_file_bytes = 67108864
max_dimension = 16384
max_pixels = 100000000
debounce_ms = 350
inactivity_timeout_minutes = 20 # 1..=1440; cannot be disabled
max_unattended_captures = 10 # 1..=100; cannot be disabled
notify_terminal_on_auto_pause = false
```

Optional pause notifications use Herdr inside a managed pane, or OSC 9 in a
verified standalone Ghostty or iTerm2 session. The persistent in-app state is
authoritative and never contains screenshot filenames or content. Linux reports
that the Screenshot Inbox is available on macOS only.

## Native submission with Herdr

In a Herdr-managed pane, Proqi discovers verified adjacent coding agents above,
below, left, and right. `s` submits selected thoughts in visible order and
removes unchanged sources only after a matching durable acceptance; `S` keeps
them. Palette actions can select or submit the entire board. One target submits
directly; several use the directional chooser.

Submission works while the receiver is busy; its harness decides whether the
prompt steers the current turn or queues a follow-up. Ambiguity, timeout,
rejection, or receipt mismatch leaves the board unchanged. Avoid simultaneous
senders when an exact prompt boundary is critical.

Proqi never invokes a shell, injects raw keys, reads the conversation, or waits
for the response. Herdr is optional.

| Harness | Herdr protocol 19 qualification |
| --- | --- |
| Claude Code, Codex | Supported |
| Pi, Hermes | Supported with official integration |
| OpenCode, Kilo | Conditional; see qualification notes |
| Cline | Deferred |

Details: [OpenCode](context/harnesses/opencode.md),
[Kilo](context/harnesses/kilo.md), [Pi](context/harnesses/pi.md), and
[Hermes](context/harnesses/hermes.md).

## JSON CLI and agent skill

The human CLI and versioned JSON interface share typed identifiers, durable
idempotency, and active-session synchronization:

```shell
proqi --json capabilities
proqi --json sessions list
printf '%s' 'Review this.' | proqi --json thoughts add <session-id>
proqi --json thoughts list <session-id>
proqi --json thoughts send <source> <thought-id> <destination> --remove
proqi --json thoughts undo <session-id>
```

Exact replacement, collapse, move, delete, redo, and inspection are also
available. Cross-session removal happens only after destination durability.
Pre-1.0 consumers must discover capabilities instead of assuming compatibility.

The explicit-invocation [Proqi skill](skills/proqi/SKILL.md) uses this contract;
it never scrapes the TUI or reads every board automatically:

```shell
npx skills add oborchers/proqi --skill proqi -g --agent codex --agent claude-code
```

The skill does not install the Proqi executable. Verify the binary with
`proqi --json capabilities`. For content-redacted failure investigation:

```shell
npx skills add oborchers/proqi --skill proqi-debug -g
proqi diagnostics keypress
```

## Privacy, durability, and recovery

Thoughts, attachments, settings, SQLite state, and bounded content-redacted logs
stay in platform-native local application directories. There is no telemetry,
cloud sync, collaboration service, or automatic upload.

The footer reports pending, saved, or failed durability. Failed state remains in
memory, blocks destructive exit, and can be retried or exported atomically.
SQLite uses WAL, full synchronous durability, migrations with backups, integrity
checks, and restart-persistent editor and board history.

```shell
proqi doctor
proqi diagnostics collect --output proqi-diagnostics.json
```

Diagnostics are read-only or content-redacted and never upload. Review bundles
before sharing. See [SECURITY.md](SECURITY.md).

Eligible interactive release builds perform a bounded background update check
against the verified installation channel. Requests contain no thoughts,
sessions, paths, clipboard data, terminal content, or installation ID. Disable
automatic checks with `check_for_updates = false`; explicit checks remain
available through `proqi update check --json` and the command palette.

## Configuration

Optional `config.toml` lives in Proqi's platform-native configuration directory:

```toml
check_for_updates = true
show_session_id = false
smart_lists = true
list_indent_width = 2
theme = "auto" # auto, light, dark, limited, or a bounded local theme file
density = "comfortable" # or compact

[keybindings]
new = "n"
edit = "e"
delete = "d"
copy = "y"
cut = "x"
submit_remove = "s"
submit_keep = "S"
undo = "u"
focus_up = "k"
focus_down = "j"
range_up = "K"
range_down = "J"
collapse = "c"
select = " "
select_all = "a"
range_select = "v"
search = "/"
commands = ":"
help = "?"
quit = "q"
screenshot_inbox = "i"
```

Semantic theme colors can be overridden with `#RRGGBB`; unsafe contrast is
rejected before startup. Start from [proqi-dark.toml](docs/themes/proqi-dark.toml).
Additional invocation roots must declare a local path, definition kind, harness,
and project or global scope; remote roots are rejected.

## Compatibility and contributing

Proqi supports macOS and x86-64 GNU/Linux. The release provides native macOS
binaries, an x86-64 Linux archive, and an `amd64` Debian package. Only the latest
stable `0.x` release is supported; the crates.io package distributes the binary,
not a stable Rust library API.

Proqi is MIT licensed. Focused issues and pull requests are welcome; read
[CONTRIBUTING.md](CONTRIBUTING.md), [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md),
[PRODUCT.md](context/PRODUCT.md), and [ARCHITECTURE.md](context/ARCHITECTURE.md)
before changing public behavior or durable contracts.

```shell
cargo build --locked
cargo run --bin proqi
cargo xtask check # canonical local gate
```

The demo is recorded from the real release binary with
`./scripts/readme-demo.sh record`; the public assets gate validates its fixtures,
dimensions, links, and privacy.
