<p align="center">
  <img src="https://raw.githubusercontent.com/oborchers/proqi/main/assets/proqi-logo.png" width="172" alt="Proqi logo">
</p>

<h1 align="center">Proqi</h1>

<p align="center">
  <strong>The agent-optimized scratchpad for follow-up prompts.</strong><br>
  Keep prompts editable beside a coding-agent session, then copy or submit them when the agent is ready.
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
  <img src="https://raw.githubusercontent.com/oborchers/proqi/main/assets/proqi-demo.gif" width="1000" alt="Proqi browsing rich prompt context, deleting a temporary thought, creating and editing a prompt, reordering it, and copying a multi-selection">
</p>

Proqi is the thoughtpad for humans working with coding agents. Follow-up prompts
accumulate while those agents are working. Keeping them in a generic editor
means another application, more context switching, and no direct relationship
to the active agent runtime. Proqi provides a local terminal scratchpad beside
the session. Existing and new prompts remain editable until you copy one or
submit it through a verified integration.

Proqi works standalone in any supported terminal. It works best with
[Herdr](https://github.com/herdrdev/herdr), where it can discover verified
adjacent coding agents and submit the selected thought directly without the
manual clipboard handoff.

<p align="center">
  <img src="https://raw.githubusercontent.com/oborchers/proqi/main/assets/proqi-problem.svg" width="1000" alt="Proqi keeps follow-up prompts editable and sends the selected prompt toward a coding-agent terminal">
</p>

## What Proqi gives you

| Need | Proqi behavior |
| --- | --- |
| Capture the next thought | Paste in board mode or click `+ New thought` to create and focus it |
| Edit without compromise | Multiline Unicode editing, selection, logical-line deletion, and persistent editor undo |
| Keep prompts flexible | Reorder or duplicate by keyboard, mouse, or drag, then collapse long context without changing its content |
| Act on several prompts | Select thoughts with `Space`, then copy, cut, delete, collapse, or submit them as one ordered prompt |
| Survive interruption | Autosave, exact resume guidance, session search, recovery export, and undo after restart |
| Work beside any agent | Native copy and non-destructive cut work without an integration or account |
| Pass local context | Drop files as paths or paste clipboard images into private session storage |
| Correct the wrong board | Send a thought to another named Proqi session, optionally removing it after delivery |
| Submit directly when verified | Optional Herdr delivery to eligible coding-agent panes in all four directions |
| Automate safely | Versioned JSON, typed identifiers, idempotent mutations, and an explicit-invocation Proqi skill |

The board is a responsive one-column interface. Thoughts use their natural
height until the viewport cap is reached. Rapid pane resizing preserves focus,
the logical cursor, selection, and valid scroll bounds. Every core action has a
keyboard and mouse path. The footer assigns separate responsive rows to
transient status, the session name, board state, actions, and verified agents,
so a long session name cannot cover an error or remove the rename target.

Images, files, and large pastes stay compact while editing. Proqi renders
`[Image 1]`, `[File 1]`, or
`[Pasted text · 84 lines · 5,812 characters]` while preserving the exact path or
text for copy, export, undo, resume, and submission.

## Install

### Homebrew

Homebrew is the recommended installation on macOS and supported Linux systems:

```shell
brew install oborchers/tap/proqi
```

Homebrew may ask you to trust the individual formula because personal taps
contain executable Ruby definitions. Trust only Proqi when whole-tap trust is
unnecessary:

```shell
brew trust --formula oborchers/tap/proqi
```

Upgrade explicitly with:

```shell
brew upgrade --formula oborchers/tap/proqi
```

The formula installs immutable prebuilt artifacts, shell completions, and one
native `proqi` executable. It performs no network activity during `post_install`
and includes no independent self-updater.

### Cargo

Users who already have Rust 1.88 or newer can build and install the Proqi binary
from crates.io:

```shell
cargo install proqi --locked
```

This compiles Proqi from source. The published crate exists to distribute the
`proqi` executable; it does not establish a supported Rust library API. Cargo
installations do not receive automatic package-manager updates from Proqi.

### Debian and Ubuntu

The release provides one `amd64` package for x86-64 Debian and Ubuntu systems
with glibc 2.35 or newer. Download the package and its checksum from the latest
GitHub Release, verify the exact bytes, then install the local file:

```shell
curl -fLO https://github.com/oborchers/proqi/releases/latest/download/proqi_amd64.deb
curl -fLO https://github.com/oborchers/proqi/releases/latest/download/proqi_amd64.deb.sha256
sha256sum --check proqi_amd64.deb.sha256
sudo apt install ./proqi_amd64.deb
```

There is no Proqi APT repository. `apt update` will not discover new Proqi
versions. To upgrade, download the newest `.deb` and install the local file
again. Remove only package-owned files with:

```shell
sudo apt remove proqi
```

Removal deliberately preserves local Proqi sessions, configuration, and other
state in the user's platform-native application directories. Proqi never runs
`sudo`, `apt`, `dpkg`, or `cargo install` as an implicit update action. Linux
ARM packages are not provided.

### Standalone archives

Each [GitHub Release](https://github.com/oborchers/proqi/releases/latest)
contains checksummed archives for exactly these targets:

| Platform | Target | Archive |
| --- | --- | --- |
| Apple silicon macOS | `aarch64-apple-darwin` | `proqi-aarch64-apple-darwin.tar.gz` |
| Intel macOS | `x86_64-apple-darwin` | `proqi-x86_64-apple-darwin.tar.gz` |
| x86-64 GNU Linux | `x86_64-unknown-linux-gnu` | `proqi-x86_64-unknown-linux-gnu.tar.gz` |

The GNU/Linux archive supports glibc 2.35 or newer. Release candidates are
built on Ubuntu 22.04, checked for a `GLIBC_2.35` symbol ceiling, and started
from the final archive on Ubuntu 22.04, Debian bookworm, and Ubuntu 24.04.

Verify the adjacent `.sha256` file before extracting. GitHub CLI can also
verify the signed build provenance:

```shell
gh attestation verify proqi-aarch64-apple-darwin.tar.gz --repo oborchers/proqi
```

Keep `proqi-installation.json` beside a manually installed archive binary so
Proqi can accurately identify the standalone installation. Archive builds have
no Node, Python, JVM, or other language runtime dependency.

## Start a board

```shell
proqi
```

Paste text into the empty board, press `Esc` to return from editing, and press
`?` for contextual shortcuts. Changes are autosaved. On exit, Proqi prints the
exact command needed to resume that session.

`Primary` means `Command` on macOS and `Ctrl` on Linux. Portable
fallbacks remain available when a terminal cannot report a modifier.

### Board controls

| Input | Action |
| --- | --- |
| Paste or click `+ New thought` | Create and focus a thought |
| `j` / `k` or arrows | Focus the next or previous thought, including `+ New thought` |
| `Enter` or `e` | Edit the focused thought |
| `Meta+J` / `Meta+K`, `Meta+Shift+↓` / `Meta+Shift+↑`, or drag | Reorder the focused thought |
| `y` or `Primary+C` | Copy the complete thought |
| `x` or `Primary+X` | Cut only after confirmed clipboard success |
| `Space` | Add or remove the focused thought from the multi-selection |
| `Shift+↑` / `Shift+↓`, or `K` / `J` | Extend or shrink one anchored contiguous range |
| `v`, then arrows, `j` / `k`, or a thought click | Latch modifier-free contiguous range selection |
| `Primary+D` | Duplicate the focused thought or selection below its source range |
| `s`, then direction when needed | Submit selected thoughts and remove only after acceptance |
| `S`, then direction when needed | Submit selected thoughts and keep them |
| `u` | Undo the latest board operation |
| `c` | Collapse or expand long context |
| `/` | Search thought content |
| `:` | Search commands |
| `?` | Open contextual help |

### Editor controls

| Input | Action |
| --- | --- |
| `Esc` | Return to the board |
| `Primary+A` | Select all text |
| `Primary+U` | Delete one logical line |
| `Primary+Z` | Undo an edit |
| `Shift+Primary+Z` | Redo an edit |
| `Primary+V` | Read the native clipboard |
| `Enter` | Continue `-`, `*`, `+`, ordered, and task list items; exit an empty top-level item |
| `↑` / `↓` twice at a boundary | Return to the board and focus the adjacent thought |

Mouse users can focus and edit thoughts, place the cursor, drag selections,
double-click words, triple-click logical lines, extend text or board ranges with Shift-click, scroll,
reorder thoughts, click controls, use help, and choose verified Herdr targets.
Holding the final click while dragging extends by complete words or logical
lines. Moving onto folded context selects its complete canonical range.
`Enter` expands the fold, while typing or deletion replaces it atomically.
Press `Esc`, open `:`, and choose `Insert plain newline` to bypass list
continuation without relying on a terminal modifier; mouse users can open the
palette directly while editing. Set `smart_lists = false` to keep every ordinary
editor `Enter` plain.

## Resume and organize sessions

Every process owns one exclusive session lease. Different sessions can run at
the same time, but two processes cannot silently edit the same session.

```shell
proqi                         # open a fresh board
proqi -c                      # continue the latest inactive board here
proqi -r                      # open the session browser
proqi -r <id-or-name>         # resume one exact session
proqi sessions                # list and search sessions
proqi sessions rename <id> "release review"
proqi sessions trash <id>
proqi sessions restore <id>
```

The session browser searches optional names, directory context, and thought
content. It ranks the current directory without hiding other results and shows
active, resumable, recovered, and trashed states in narrow and wide layouts.

## Files, images, and clipboard safety

A terminal file drop normally arrives as bracketed text. Proqi converts it only
when the complete payload resolves unambiguously to existing absolute files.
Quoted paths, POSIX shell-escaped paths, local file URLs, multiple paths, and
Unicode names are supported. Ordinary prompt text stays exact. Dropped files
are never read, copied, uploaded, or analyzed.

When the native clipboard contains raw image pixels, `Primary+V` validates the
image, atomically writes a private PNG below the current session data directory,
and inserts its path. A failure inserts nothing. Copy has an OSC 52 fallback,
but cut never deletes after an unconfirmed OSC 52 write.

## Best with Herdr

Inside a Herdr-managed pane, Proqi can discover coding agents above, below,
left, and right. A delivery control appears only after workspace, tab, geometry,
edge overlap, agent kind, readiness, and protocol capability have been verified
through Herdr's structured interface. Established agents also require an exact
session identity. Codex, Kilo, and OpenCode may appear provisionally
before they have a session. If Herdr acknowledges the first prompt before a
session hook reports an identity, Proqi accepts the matching receipt and
immediately refreshes discovery without sending the prompt again.

Compatibility was qualified against Herdr 0.8.0/protocol 19:

| Harness | Status | Session behavior | Setup and known limits |
| --- | --- | --- | --- |
| Claude Code | Supported | Established identity | Uses Herdr's established-session path |
| Codex | Supported | Provisional first prompt, then established | Same-pane replacement race applies before the first session exists |
| OpenCode | Conditional | Provisional first prompt; resume may remain sessionless | Install the official OpenCode integration; resumed identity reporting is blocked by [Herdr issue #2548](https://github.com/herdrdev/herdr/issues/2548) |
| Cline | Deferred | Not supported | Requalify after Herdr provides a stable Cline state/session hook |
| Kilo | Conditional | Provisional first prompt, then established | Install the official Kilo integration; the protocol-19 provisional replacement race remains |
| Pi | Supported | Established after startup trust | Install the official Pi integration |
| Hermes | Supported | Established before eligibility | Install the official Hermes integration |

The detailed qualification records are
[OpenCode](context/harnesses/opencode.md), [Kilo](context/harnesses/kilo.md),
[Pi](context/harnesses/pi.md), and [Hermes](context/harnesses/hermes.md).

Both actions use Herdr's semantic prompt operation. When several thoughts are
selected, Proqi sends one prompt containing their exact content in board order,
separated by one blank line:

- `s Submit` submits immediately and deletes the source thoughts only after an
  accepted matching receipt.
- `S Submit & keep` submits immediately and retains the source thoughts.

Both actions work while the receiving agent is working. The receiving harness
decides whether that input steers the current turn or becomes follow-up input.

Herdr protocol 19 does not guarantee a distinct prompt boundary when another
sender submits at nearly the same time. Concurrent inputs may therefore merge
at the receiving harness. Avoid simultaneous submissions when preserving that
boundary is critical. Protocol 19 also cannot distinguish replacement of one
supported sessionless harness by another instance of the same kind in the same
pane during the narrow interval between Proqi's revalidation and delivery.

Ambiguity, timeout, rejection, receipt mismatch, or protocol mismatch always
leaves the thought unchanged. Proqi never invokes a shell, injects raw keys,
reads the conversation, or waits for the agent response. Herdr is optional. The
complete standalone workflow works without it.

## JSON CLI and Proqi skill

The human CLI and versioned JSON contract use the same typed identifiers and
durability rules:

```shell
proqi --json capabilities
proqi --json sessions list
printf '%s' 'Review this exact prompt.' | proqi --json thoughts add <session-id>
proqi --json thoughts list <session-id>
proqi --json thoughts inspect <session-id> <thought-id>
printf '%s' 'Exact replacement.' | proqi --json thoughts replace <session-id> <thought-id> --revision-id <revision-id> --expected-sha256 <digest>
proqi --json thoughts collapse <session-id> <thought-id> --collapsed true
proqi --json thoughts move <session-id> <thought-id> <zero-based-position>
proqi --json thoughts send <source-session> <thought-id> <destination-session>
proqi --json thoughts send <source-session> <thought-id> <destination-session> --remove
proqi --json thoughts delete <session-id> <thought-id>
proqi --json thoughts undo <session-id>
```

Mutations accept typed operation or revision IDs for durable idempotency. Reads
synchronize with a compatible active owner before inspecting SQLite, and remain
available from the last durable state of a legacy owner. Rename, add, exact replacement,
collapse, move, delete, undo, and redo commands aimed at an active session are
forwarded through its verified local owner channel on macOS and Linux. They
never write around the owning reducer. An external replacement is an ordinary
editor revision, so it participates in persistent undo and redo. It requires
the `content_sha256` returned by list or inspect unless the caller deliberately
uses `--force`. A thought with an in-flight agent submission is locked against
both TUI and CLI mutation. Unsupported or unverifiable forwarding returns
`session_busy`.

Cross-session send accepts a canonical session identifier or unique exact name.
It copies canonical content and folded-context annotations. With `--remove`,
Proqi commits the destination first and removes the source only after its durable
receipt. Destination failure leaves the source unchanged.

[`skills/proqi/SKILL.md`](https://github.com/oborchers/proqi/blob/main/skills/proqi/SKILL.md) is an explicit-invocation skill
that discovers capabilities first, uses standard input for arbitrary content,
and never scrapes the TUI or reads every scratchpad automatically. During the
pre-1.0 series, agents must discover the installed CLI contract instead of
assuming compatibility with another Proqi version.

Install the skill globally with the canonical `skills` CLI after installing the
`proqi` binary on `PATH`:

```shell
npx skills add oborchers/proqi --skill proqi -g
```

To install explicitly for both Codex and Claude Code:

```shell
npx skills add oborchers/proqi --skill proqi -g --agent codex --agent claude-code
```

The skill does not install the Proqi executable. Verify both parts with
`proqi --json capabilities` before asking an agent to use it.

For local failure investigation, install the separate, read-only-first debug
skill:

```shell
npx skills add oborchers/proqi --skill proqi-debug -g
```

[`skills/proqi-debug/SKILL.md`](https://github.com/oborchers/proqi/blob/main/skills/proqi-debug/SKILL.md) explains the
content-redacted diagnostics bundle, the SQLite durability model, safe failure
classification, and the approval-gated process for opening a GitHub Issue. It
never authorizes direct writes to the live database or automatic uploads.
When a terminal sends an unexpected shortcut, inspect one key without opening
SQLite or a Proqi session:

```shell
proqi diagnostics keypress
```

The command reports Crossterm's raw key event and the normalized Proqi action.

## Updates and privacy

Every eligible interactive release startup checks the installable version for
its verified channel in the background. Concurrent Proqi startups share one
request and one prompt, while the next independent startup checks again.
Standalone archives use the public stable GitHub Release endpoint. Homebrew
installations use the public tap formula, so Proqi never advertises a release
before the tap can install it. Debug builds, source installations, JSON
commands, skills, and other noninteractive commands do not perform automatic
checks. Disable startup checks globally in `config.toml`:

```toml
check_for_updates = false
```

The release API request sends GitHub's release media type and API version. The
tap request reads only the public formula. Both may send an optional safe ETag
and a bounded `proqi/<version>` User-Agent. Neither sends thoughts, session
identifiers, paths, clipboard data, terminal content, installation ID, or
runtime state. Run an explicit bounded check with:

```shell
proqi update check --json
```

The searchable command palette also includes **Check for updates**. An explicit
check is user-authorized even when automatic startup checks are disabled.

Homebrew installations on macOS and Linux can choose **Update and restart all
sessions**, **Not now**, or **Skip this version**. Proqi coordinates one update
across every verified active instance that shares the installation. Each
participant durably saves before the one shell-free
`brew upgrade --formula oborchers/tap/proqi` invocation. A failed save, refused
participant, timeout, or installer failure cancels safely. After success, each
participant independently replaces its process image and resumes its ordinary
session. Partial restart failures remain visible and recoverable.

The coordination path is designed and deterministically tested for the ordinary
case of 10 to 15 concurrent Proqi instances. Notification, dismissal, and skip
state are installation-wide so multiple boards cannot compete for attention.
**Not now** lasts until the next successful eligible startup check. **Skip this
version** remains active until a newer release exists.

Standalone archives receive verified release instructions. Proqi does not
replace a standalone executable or promise a same-pane restart. Durable boards
resume on the next normal start after the user replaces the archive.

## Persistence, recovery, and security

The footer distinguishes pending, saved, and failed persistence. A failed write
stays in memory, blocks destructive exit, and offers retry or an atomic private
recovery export. Session trash is recoverable. Permanent pruning is separate
and explicit.

SQLite uses WAL, full synchronous durability, bounded contention retry,
forward-only migrations, backups before migration, integrity checks, and
exclusive session leases. Persistent editor revisions and board inverse
operations make undo and redo survive a restart.

Proqi stores content only in platform-native local application directories.
Diagnostics are structured, content-redacted, user-private, and bounded. Each
running instance retains at most five 1 MiB JSONL segments. Inactive logs are
pruned toward a 20 MiB installation-wide ceiling. Active instance logs are
never removed merely to satisfy that ceiling.

Create a local support bundle explicitly with:

```shell
proqi diagnostics collect --output proqi-diagnostics.json
proqi doctor
proqi --json doctor
```

Doctor performs content-redacted, read-only health checks without initializing,
migrating, or repairing state. Diagnostic collection never uploads anything
and never overwrites an existing file.
Review the bundle before sharing it. See [SECURITY.md](https://github.com/oborchers/proqi/blob/main/SECURITY.md) for the
support policy and private vulnerability reporting process.

## Configuration

An optional `config.toml` lives in the platform-native Proqi configuration
directory. The default `auto` theme preserves the terminal palette and applies
Proqi's adaptive mint accents. The built-ins are `auto`, `light`, `dark`, and
`limited`. Every semantic color can also be overridden while portable editor
shortcuts remain available:

```toml
check_for_updates = true
show_session_id = false # opt in to the complete ses_... value in the footer
smart_lists = true
theme = "auto"
density = "comfortable" # or "compact"

[theme_overrides]
# link = "#7DD3FC"
# annotation = "#70D69B"
# focused_surface = "none"

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
range_select = "v"
search = "/"
commands = ":"
help = "?"
quit = "q"
```

The command palette always offers **Copy session ID** and **Copy resume
command**, even when the identifier is hidden in a narrow footer. When
`show_session_id = true`, the muted identifier appears beside the session name
only if its complete canonical value fits; clicking it copies that complete
value while the name remains a rename target.

To install a complete local theme, set `theme` to an absolute TOML path or a
path relative to `config.toml`:

```toml
theme = "themes/my-theme.toml"

# Optional final overrides take precedence over the theme file.
[theme_overrides]
accent = "#70D69B"
```

Theme files use semantic roles rather than widget-specific colors. Start with
the checked-in [`proqi-dark.toml`](https://github.com/oborchers/proqi/blob/main/docs/themes/proqi-dark.toml)
example. Colors must use `#RRGGBB`; `focused_surface` additionally accepts
`"none"`. Proqi accepts theme-file symlinks, bounds local files to 64 KiB, and
does not fetch remote themes. A custom theme that fails Proqi's WCAG contrast
checks is rejected before the terminal interface starts. Limited-color
terminals use the safe terminal-native fallback because custom RGB colors
cannot be represented faithfully.

`brew uninstall proqi` removes the formula but deliberately leaves user data.
Back up or remove the platform-native Proqi data, configuration, and cache
directories separately only when you no longer need their sessions.

## Release and support policy

Proqi is open-source software under the [MIT License](https://github.com/oborchers/proqi/blob/main/LICENSE). During the
`0.x` series, only the latest stable release is supported, and JSON consumers
must use capability discovery. There is currently no cloud synchronization,
collaboration service, plugin system, Homebrew Core formula, npm package,
Python package, container image, or telemetry. The prepared crates.io package
distributes the binary and does not define a supported Rust library API.

Release archives include SHA-256 checksums, SPDX 2.3 JSON SBOMs, required
third-party notices, shell completions, and GitHub OIDC-backed provenance and
SBOM attestations. The public
[`oborchers/homebrew-tap`](https://github.com/oborchers/homebrew-tap) formula is
updated only after all referenced release assets are available and verified.

Issues and focused pull requests are welcome. Read
[CONTRIBUTING.md](https://github.com/oborchers/proqi/blob/main/CONTRIBUTING.md) and
[CODE_OF_CONDUCT.md](https://github.com/oborchers/proqi/blob/main/CODE_OF_CONDUCT.md) before changing product scope,
durable storage, public behavior, or architecture.

## Development

Build from source with the checked-in toolchain:

```shell
git clone https://github.com/oborchers/proqi.git
cd proqi
cargo build --locked
cargo run --bin proqi
```

The repository has one canonical automation surface:

```shell
cargo xtask setup              # verify required local developer tools
cargo xtask format             # apply formatting
cargo xtask source-limits      # enforce the 500-line source-file ceiling
cargo xtask architecture       # enforce inward dependency boundaries
cargo xtask assets             # verify public assets and recording fixtures
cargo xtask check              # formatting, architecture, Clippy, docs, and tests
cargo xtask test-pty           # real terminal scenarios on macOS
cargo xtask coverage           # enforce the line-coverage floor
cargo xtask audit              # advisories, licenses, sources, and dependencies
cargo xtask package            # archive and isolated installed-product contract
cargo xtask release-rehearsal  # non-publishing host release rehearsal
```

Clippy warnings are denied. Rust functions are capped at 80 lines, cognitive
complexity at 25, and nesting depth at 4. Every first-party source file is
capped at 500 physical lines. CI covers Linux and macOS, plus macOS PTY tests
and the three-target release matrix.

Enable the optional local hook explicitly with `cargo xtask install-hooks`.
Builds never change Git configuration automatically.

The README demo uses the real release binary and deterministic temporary state:

```shell
brew install asciinema agg fontconfig
./scripts/readme-demo.sh record
```

The recorder uses `Meslo LG M DZ for Powerline`, available from the
[Powerline fonts repository](https://github.com/powerline/fonts/tree/master/Meslo%20Dotted).
It verifies the exact family before rendering, so a missing font cannot
silently change the published demo typography.

The social preview is generated from its checked-in SVG source with:

```shell
brew install librsvg
./scripts/social-preview.sh
```

[`context/PRODUCT.md`](https://github.com/oborchers/proqi/blob/main/context/PRODUCT.md) defines user-visible behavior.
[`context/ARCHITECTURE.md`](https://github.com/oborchers/proqi/blob/main/context/ARCHITECTURE.md) defines technical
boundaries and durable invariants.
