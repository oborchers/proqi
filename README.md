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

[Why Proqi](#do-you-hate-this-editor) ·
[Workflow](#one-board-many-prompts) ·
[Install](#install) ·
[Controls](#board-controls) ·
[Screenshots](#screenshot-inbox-on-macos) ·
[Herdr](#native-submission-with-herdr) ·
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
power users.** Capture independently; edit, select, duplicate, reorder, recover,
and discover local skills and commands later.

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

Install the latest supported release with one command. This downloads the
release-attached installer and its checksum separately, verifies the installer,
then lets that verified installer select and verify the exact native archive:

```shell
sh -c 'set -eu
version=${1:-latest}
case "$version" in
  latest) release=latest/download ;;
  v*)
    numbers=${version#v}; case "$numbers" in *[!0-9.]*|.*|*.|*..*) printf "invalid Proqi version\n" >&2; exit 1 ;; esac
    saved_ifs=$IFS; IFS=.; set -- $numbers; IFS=$saved_ifs; test "$#" = 3 || { printf "invalid Proqi version\n" >&2; exit 1; }
    for component in "$@"; do case "$component" in ""|*[!0-9]*|0[0-9]*) printf "invalid Proqi version\n" >&2; exit 1 ;; esac; done
    release="download/$version"
    ;;
  *) printf "invalid Proqi version\n" >&2; exit 1 ;;
esac
temporary=$(mktemp -d "${TMPDIR:-/tmp}/proqi-bootstrap.XXXXXX")
trap '\''rm -rf "$temporary"'\'' EXIT HUP INT TERM
base="https://github.com/oborchers/proqi/releases/$release"
for file in proqi-installer.sh.sha256 proqi-installer.sh; do
  case "$file" in *.sha256) maximum=512 ;; *) maximum=131072 ;; esac
  curl --fail --silent --show-error --location --proto "=https" --proto-redir "=https" --tlsv1.2 --connect-timeout 10 --max-time 60 --max-redirs 3 --retry 2 --max-filesize "$maximum" --output "$temporary/$file" "$base/$file"
done
test "$(wc -l < "$temporary/proqi-installer.sh.sha256" | tr -d " ")" = 1
record=$(cat "$temporary/proqi-installer.sh.sha256")
set -f; set -- $record; set +f
test "$#" = 2 && test "$2" = proqi-installer.sh && test "${#1}" = 64
case "$1" in *[!0-9a-f]*) printf "invalid installer checksum\n" >&2; exit 1 ;; esac
expected=$1
if command -v sha256sum >/dev/null 2>&1; then output=$(sha256sum "$temporary/proqi-installer.sh"); elif command -v shasum >/dev/null 2>&1; then output=$(shasum -a 256 "$temporary/proqi-installer.sh"); else printf "sha256sum or shasum is required\n" >&2; exit 1; fi
actual=${output%% *}; test "$actual" = "$expected" || { printf "installer checksum verification failed\n" >&2; exit 1; }
sh "$temporary/proqi-installer.sh" --version "$version"' sh latest
```

Replace the final `latest` with an exact stable tag from the Releases page to
require that version. The default destination is `$HOME/.local/bin`; set
`PROQI_INSTALL_DIR` to another absolute directory below `$HOME`. The installer
never uses `sudo` or modifies `PATH`. If the destination is not already on
`PATH`, it prints the required addition.

Homebrew remains supported:

```shell
brew install oborchers/tap/proqi
```

Scope Homebrew trust to this formula:

```shell
brew trust --formula oborchers/tap/proqi
brew upgrade --formula oborchers/tap/proqi
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

A genuinely empty board opens with `+ Start typing`. Type or paste immediately
to create the first thought, or click the insertion row to reveal the ordinary
empty editor first. Nothing is saved until content is produced. Press `Esc` to
use Board controls instead. Returning focus to the pane does not override that
Board choice.

`Primary` means logical `Cmd` (`Super` or `Meta`) on macOS and logical `Ctrl`
elsewhere. Proqi receives modifiers only after the operating system, keyboard
remapper, and terminal have handled the key. Raw `Ctrl` is not a second Primary
modifier on macOS.

The tables below describe the factory map. The complete stable action and
context inventory is in [context/KEYMAP_ACTIONS.md](context/KEYMAP_ACTIONS.md).
Help and footer labels always show the bindings resolved from the active
configuration.

### Board controls

| Input | Action |
| --- | --- |
| `n`, `Enter` on `+ New thought`, paste, or click | Create a thought |
| `Primary+V` / `p` with no selection | Paste exactly as a new thought |
| `j` / `k` or arrows | Focus next / previous; twice at a blocked bottom / top edge creates there |
| `Ctrl+↓` / `↑` or `Ctrl+j` / `k` | Focus the last / first live thought without wrapping |
| macOS `Ctrl+N` / `Ctrl+Shift+N`; elsewhere `Alt+↓` / `↑` or `Alt+j` / `k` | Insert a blank below / above the focused thought and edit it |
| `Page Up` / `Page Down` | Move five thoughts previous / next |
| `Enter` or `e` | Edit |
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
| `f` | Clean up spacing in the focused thought |
| `c`; `/`; `:`; `i`; `?` | Collapse; search; commands; Screenshot Inbox; help |
| `Esc`; `Primary+Q` / `q` | Clear selection; exit after durable flush |

### Editor controls

| Input | Action |
| --- | --- |
| `Esc` | Return to the board |
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
| Rename and Browser rename | Type and use text cursor, `Backspace`, or `Delete`; `Enter` confirms; `Esc` cancels |
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
proqi --json thoughts send <source> <thought-id> <destination> --remove
```

The [Proqi skill](skills/proqi/SKILL.md) uses it without scraping the TUI:

```shell
npx skills add oborchers/proqi --skill proqi -g --agent codex --agent claude-code
```

The skill does not install the Proqi executable. Run `capabilities` first.

For read-only-first failure investigation:

```shell
npx skills add oborchers/proqi --skill proqi-debug -g
```

## Privacy, durability, and recovery

Thoughts, attachments, settings, and redacted logs stay local. No telemetry,
cloud sync, collaboration service, or upload.

The footer reports durability. Failures block destructive exit and remain
retryable/exportable. Editor and board history survive restart.

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

## Configuration

Optional platform-native `config.toml`:

```toml
check_for_updates = true
theme = "auto" # auto, light, dark, limited, or a bounded local theme file
density = "comfortable" # or compact
merge_separator = "\n\n" # one blank line between merged thoughts
mouse_capture = true # set false if your terminal/multiplexer mishandles mouse reporting

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

[keymap.macos.edit]
"submission.submit_remove" = [{ key = "Enter", modifiers = ["Super", "Alt"] }]

[keymap.portable.edit]
"submission.submit_remove" = [{ key = "F5" }]
```

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
