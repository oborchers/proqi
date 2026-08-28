# Proqi feature inventory

Status: current `v0.2.0` release candidate, reviewed 2026-08-26.

This inventory connects observable behavior to implementation and test
evidence. `Shipped` means a user can reach the behavior in the current native
binary. `Conditional` means the behavior is shipped but requires a verified
external context. `Internal` means the contract supports a user-facing feature
without being a separate advertised feature. `Not shipped` means public copy
must not imply that the behavior exists.

## Terminal board

| Capability | Status | Reachable behavior | Evidence |
| --- | --- | --- | --- |
| Fresh and resumable boards | Shipped | `proqi`, `proqi -c`, and `proqi -r [ID_OR_NAME]` open persisted local sessions. | `tests/cli_workflow.rs`, `tests/pty.rs` |
| Board and edit modes | Shipped | Users navigate thoughts in board mode and enter exact multiline editing explicitly. | `tests/ui_board/navigation.rs`, `tests/editor_contract.rs` |
| Thought creation | Shipped | `n`, the keyboard-focusable insertion row, its mouse target, board paste, and two consecutive downward navigation commands on the insertion row create thoughts. An explicitly created blank is durable. | `tests/ui_board/blank.rs`, `tests/ui_board/insertion_navigation.rs`, `tests/ui_mouse_actions.rs` |
| Exact Unicode editing | Shipped | Grapheme-aware cursor movement preserves combining marks, emoji sequences, CJK, wide cells, tabs, CRLF, and whitespace. | `tests/editor_contract.rs`, `tests/ui_board.rs` |
| Selection and cursor | Shipped | Keyboard selection, single-click placement, character dragging, double-click word selection, triple-click logical-line selection, granular dragging, Shift-click extension, wrapped cursor placement, trailing newlines, and a terminal-owned blinking cursor are supported. | `tests/editor_contract.rs`, `tests/ui_board/pointer_selection.rs`, `tests/ui_board/navigation.rs` |
| Copy, cut, and delete | Shipped | Copy preserves exact content. Cut and editor cut delete only after clipboard success. Delete retains undo history. | `tests/domain_reducer/clipboard.rs`, `tests/ui_board/clipboard.rs` |
| Multi-selection | Shipped on current main | `Space` toggles arbitrary thoughts. Shifted arrows and Shift-click extend a stable anchored range; the remappable `v` latch supplies modifier-free arrow, `j`/`k`, and click parity. Bulk copy, cut, delete, duplicate, collapse, and one-call Herdr submission preserve board order and structural undo. | `tests/ui_board/selection.rs`, `tests/ui_board/agent_selection.rs`, `tests/sqlite_store/bulk.rs`, `tests/pty.rs` |
| Reordering | Shipped | Mouse drag and `J` / `K` move one thought and wrap at board edges. Selected-block reorder is intentionally unavailable. | `tests/ui_board/navigation.rs`, `tests/ui_board.rs` |
| Persistent undo and redo | Shipped | Board and editor histories remain separate and survive process restart. | `tests/domain_reducer/history.rs`, `tests/sqlite_store/core.rs` |
| Collapse and long content | Shipped | `c` collapses long thoughts without changing canonical content. Scrolling is bounded, advances by wrapped row, and keeps focus and the insertion row reachable. | `tests/ui_board/composition.rs`, `tests/ui_board/navigation.rs` |
| Search and command discovery | Shipped | Thought search, a searchable command palette, contextual help, keyboard control, and mouse control are available. | `tests/ui_board.rs`, `tests/ui_board/snapshots.rs`, `tests/ui_mouse_actions.rs` |
| Responsive rendering | Shipped | One-column layout reflows across narrow, wide, tall, shallow, and repeated-resize viewports without mutating content or logical cursor state. | `tests/ui_board/composition.rs`, `tests/pty.rs` |
| Theme and focus accessibility | Shipped | Auto, dark, light, and limited-color modes plus bounded local semantic theme files and inline overrides use non-color focus cues, terminal-aware surfaces, and enforced contrast pairs. | `src/ui/theme.rs`, `src/ui/theme/`, `src/adapters/terminal/settings.rs`, `tests/ui_board/snapshots.rs` |
| Keyboard and mouse parity | Shipped | Core creation, focus, edit, search, help, recovery, footer, and drag interactions have both input paths where terminals expose them. | `tests/ui_mouse_actions.rs`, `tests/ui_board.rs` |
| URL presentation | Shipped | Explicit HTTP and HTTPS ranges receive accent and underline styling without changing content. Activation remains a terminal capability. | `tests/ui_board/composition.rs` |

## Content provenance and recovery

| Capability | Status | Reachable behavior | Evidence |
| --- | --- | --- | --- |
| Bracketed and large paste | Shipped | A paste is one exact undo unit. Large pastes use an atomic folded presentation while canonical content stays intact. | `tests/editor_contract.rs`, `tests/ui_board/annotations.rs`, `tests/pty.rs` |
| File paths and file drop | Shipped | Existing absolute paths, quoted paths, POSIX shell-escaped paths, file URLs, and multi-file drops become attachment annotations without losing the exact path. | `src/adapters/terminal/path_import.rs`, `tests/pty/path_drop.rs` |
| Clipboard images | Shipped | Native clipboard pixels are materialized as private PNG files and inserted as atomic image annotations. | `src/adapters/attachment/mod.rs`, `tests/ui_board/clipboard.rs` |
| Folded annotations | Shipped | Images, files, and large pasted text render as accent placeholders with atomic navigation, selection, deletion, and restart-safe undo. | `tests/ui_board/annotations.rs`, `tests/sqlite_store/core.rs` |
| Native clipboard with fallback | Shipped | Native text copy and paste are preferred. Bounded OSC 52 is the copy fallback where supported. Failure remains non-destructive and visible. | `src/adapters/clipboard/mod.rs`, `tests/ui_board/clipboard.rs` |
| Autosave truthfulness | Shipped | Pending, durable, failed, and retry states reflect acknowledgement from persistence rather than optimistic UI state. | `tests/ui_board/durability.rs`, `src/adapters/terminal/persistence.rs` |
| Recovery export | Shipped | Failed in-memory state can be exported atomically to a private recovery file without silently claiming durability. | `tests/recovery_export.rs` |
| Crash recovery | Shipped | Acknowledged paste and mutations survive forced termination. Uncommitted SQLite writes roll back. Session leases release on process death. | `tests/pty/shutdown.rs`, `tests/sqlite_store/concurrency.rs`, `tests/runtime_coordination.rs` |

## Sessions and persistence

| Capability | Status | Reachable behavior | Evidence |
| --- | --- | --- | --- |
| Session browser | Shipped | Search uses optional name, launch path, and thought content. Results retain recency and directory context and show active, resumable, recovered, and trashed states in wide and narrow layouts. | `tests/ui_session_browser.rs`, `tests/snapshots`, `tests/pty.rs` |
| Session naming | Shipped | Sessions can be renamed or cleared and resumed by unique name or canonical `ses_` identifier. | `tests/cli_workflow.rs`, `tests/ui_board/navigation.rs` |
| Debug session identity | Shipped | `show_session_id = true` adds the complete muted canonical identifier beside the footer name only when it fits. Its mouse target and the always-available palette actions copy the ID or exact resume command without durable mutation. | `tests/ui_board/session_navigation.rs`, `tests/ui_board/palette.rs`, `tests/ui_mouse_actions.rs` |
| Trash and pruning | Shipped | Session trash is recoverable. Permanent pruning requires an explicit confirmation flag. | `tests/cli_workflow.rs`, `tests/sqlite_store/core.rs` |
| Local SQLite durability | Shipped | Bundled SQLite uses WAL, `synchronous=FULL`, forward migrations, backups, integrity checks, derived search indexes, and typed identifier BLOBs. | `tests/sqlite_store.rs`, `tests/sqlite_store/recovery.rs` |
| Multiple instances | Shipped | Different sessions may be active concurrently. One authoritative lease prevents silent dual editing of the same session. | `tests/runtime_coordination.rs`, `tests/pty/active_control.rs` |
| Active-session CLI forwarding | Shipped on current main | Reads synchronize with the owner. Rename, add, exact replacement, collapse, move, delete, undo, and redo travel through its verified reducer. External replacement participates in editor undo, and in-flight submission locks are enforced. | `tests/pty/active_control.rs`, `tests/control_contract.rs`, `src/ui/app/control.rs` |
| Cross-session thought transfer | Shipped | A thought can be copied into another named or identified Proqi session and optionally removed only after destination durability. | `tests/cli_workflow.rs`, `src/ui/app/transfer.rs` |

## Adjacent agent integration

| Capability | Status | Reachable behavior | Evidence |
| --- | --- | --- | --- |
| Generic copy workflow | Shipped | Every thought can be copied or cut for native paste into any agent or application. | `tests/ui_board/clipboard.rs` |
| Herdr discovery | Conditional | In a managed Herdr pane, Proqi discovers and independently verifies adjacent coding agents in all four directions. Ready empty Codex, Kilo, and OpenCode instances may be provisional; a receipt that precedes a session hook triggers immediate rediscovery without resending. | `src/adapters/herdr`, `tests/herdr_executable.rs` |
| Submit and keep | Conditional | A verified adjacent target receives one exact prompt through Herdr's semantic command, while every selected source thought remains. Concurrent senders can expose the protocol 19 prompt-boundary limitation. | `tests/ui_board/agent.rs`, `tests/ui_board/agent_selection.rs`, `src/adapters/herdr/submission.rs` |
| Submit and remove | Conditional | The same semantic submission removes all unchanged selected sources as one operation only after the matching accepted receipt. Failure and ambiguity preserve them. | `tests/ui_board/agent.rs`, `tests/ui_board/agent_selection.rs`, `src/ui/app/agent.rs` |
| Pane presentation metadata | Conditional | A managed pane advertises display-only Proqi identity with bounded refresh and clean clearing. It does not impersonate a coding agent. | `src/adapters/herdr`, `src/adapters/terminal/runner/heartbeat.rs` |
| Conversation inspection | Not shipped | Proqi does not read agent conversations, wait for responses, or use raw key injection as a fallback. | `context/PRODUCT.md`, `src/adapters/herdr` |

## CLI and agent contract

| Capability | Status | Reachable behavior | Evidence |
| --- | --- | --- | --- |
| Capability discovery | Shipped | `proqi capabilities --json` reports schema version, identifier encoding, bounds, control protocol, transfer, update, and Herdr capabilities. | `tests/cli_workflow.rs`, `src/cli/execute/capabilities.rs` |
| Session commands | Shipped | List, search, rename, trash, restore, and prune have human and versioned JSON output. | `tests/cli_workflow.rs` |
| Thought commands | Shipped on current main | List and inspect expose a content digest. Add, exact replacement through standard input, collapse, delete, move, cross-session send, undo, and redo are scriptable. | `tests/cli_workflow.rs` |
| Typed identifiers | Shipped | `ses_`, `tht_`, `rev_`, `op_`, `ins_`, `req_`, and `sub_` preserve all UUIDv7 bits in canonical base32hex and reject wrong prefixes. | `tests/identifiers.rs`, `src/domain/identifiers.rs` |
| Idempotent mutations | Shipped | Caller-supplied `op_` identities replay matching operations and reject reuse for different input or mutation types. | `tests/cli_contract.rs`, `tests/cli_workflow.rs` |
| JSON fixtures | Shipped | Current success, error, control request, accepted receipt, and rejected receipt shapes are checked in and round-trip canonically. | `tests/fixtures`, `tests/cli_contract.rs`, `tests/control_contract.rs` |
| Machine-readable errors | Shipped | Failures use the current versioned envelope, stable current error codes, and nonzero exits. | `tests/cli_contract.rs`, `src/cli/output.rs` |
| Shell completions | Shipped | Bash, Fish, and Zsh completion output is generated by the installed binary and included in archives. | `tests/cli_smoke.rs`, `tests/package_contract.rs` |
| Dedicated Proqi skill | Shipped | `skills/proqi/SKILL.md` discovers capabilities, uses JSON and standard input, addresses explicit sessions, and never reads SQLite or TUI output. | `tests/skill_package.rs` |
| Pre-1.0 stability | Shipped policy | The JSON schema is versioned, but CLI compatibility before 1.0 is not promised. Breaking changes require release-note disclosure. | `README.md`, `context/PRODUCT.md` |

## Updates and installed product

| Capability | Status | Reachable behavior | Evidence |
| --- | --- | --- | --- |
| Explicit update check | Shipped | `proqi update check` and the command palette query the verified installable channel with bounded TLS HTTP, redirects, body, parsing, and timeout handling. | `src/adapters/update/github.rs`, `src/cli/execute/update.rs`, `src/ui/app/palette.rs` |
| Interactive startup check | Shipped | Every eligible release startup checks asynchronously. Concurrent startups coalesce into one request. Debug, test, JSON, and noninteractive paths do not check automatically. | `src/application/update.rs`, `src/adapters/terminal/runner.rs` |
| Privacy and opt-out | Shipped | `check_for_updates = false` disables automatic checks. Explicit user checks remain available. Requests contain no thought, path, session, or installation content. | `src/ui/settings.rs`, `src/adapters/update/github.rs` |
| Installation-wide election | Shipped | Shared generations and locks allow one refresh and one actionable prompt across one, ten, or fifteen concurrent startups. A later startup checks again. | `src/adapters/update/cache/tests.rs`, `src/application/update/tests.rs` |
| Homebrew coordinated update | Shipped | After explicit confirmation, every verified participant saves, one direct `brew upgrade --formula oborchers/tap/proqi` runs, active sessions are rescanned, and each process independently cleans up and uses Unix `exec` to resume in place. The prompt has reviewed wide, narrow, and shallow buffers plus keyboard and mouse coverage. | `tests/pty/update_control.rs`, `src/application/update_coordination.rs`, `src/ui/app/snapshots` |
| Failure convergence | Shipped | Failed preflight aborts before installation. Installer and coordinator failures release ready peers. Partial restart is reported without rolling back successful peers. | `src/application/update_coordination/tests.rs`, `src/adapters/terminal/runner/update_results.rs` |
| Standalone update | Shipped instructions only | Archive installs receive stable release instructions and resume normally after user-managed replacement. The binary does not replace itself. | `src/adapters/update/installation.rs`, `README.md` |
| Package contract | Shipped locally | The isolated archive smoke covers version, help, completions, JSON creation, exact Unicode, reopen, active forwarding, migration backup, newer-schema refusal, terminal restoration, fake update installation, and same-PTY replacement on macOS. | `tests/package_contract.rs`, `tests/package_contract/pty.rs` |
| crates.io binary package | Verified release contract | The registry-restricted source package has an exact member allowlist, dry-run publication, normalized-manifest inspection, checksum evidence, isolated installation, MSRV coverage, and no supported Rust library API. It becomes public only after the separately authorized registry publication succeeds. | `Cargo.toml`, `xtask/src/crate_package.rs`, `.github/workflows/ci.yml` |
| Debian package | Verified release contract | The immutable candidate builds `proqi_amd64.deb` from the exact verified Linux archive binary, derives runtime dependencies, omits maintainer scripts, preserves user state on removal, and tests the package on Ubuntu 22.04, Ubuntu 24.04, and Debian bookworm. It becomes public only with the authorized GitHub Release. There is no APT repository. | `xtask/src/debian.rs`, `xtask/src/debian_container.rs`, `.github/workflows/release-candidate.yml` |
| Release artifacts | Shipped baseline plus verified promotion contract | A protected stable tag runs the native candidate matrix once, then promotes that run's exact archives, checksums, SPDX JSON SBOMs, attestations, notices, completions, Debian package, crate evidence, and generated Homebrew formula. Public GitHub bytes are re-downloaded and compared before Homebrew is notified. | `.github/workflows/release-candidate.yml`, `.github/workflows/release.yml`, `xtask/src/release.rs`, `xtask/src/homebrew.rs` |

## Security, privacy, and operational boundaries

| Capability | Status | Reachable behavior | Evidence |
| --- | --- | --- | --- |
| User-only local state | Shipped | Ordinary startup preflights the explicit state root and its data, configuration, cache, and runtime leaves. SQLite also rejects linked database, companion, backup-directory, and backup-destination paths before following them. Private permissions remain enforced. | `tests/state_path_safety.rs`, `tests/sqlite_store/recovery.rs`, `tests/recovery_export.rs` |
| Bounded external input | Shipped | CLI thought input, control messages, HTTP bodies, diagnostics, markers, config, process output, and clipboard fallback are explicitly bounded. | ports and adapter constants, contract tests |
| Direct process execution | Shipped | Herdr, installer, and verification commands use argument vectors and optional standard input without shell interpolation. | `src/adapters/process/mod.rs`, process tests |
| Secret and content minimization | Shipped | Diagnostics are bounded and content-redacted. Release assets and public presentation checks reject private local paths. There is no telemetry. | `tests/cli_workflow.rs`, `xtask/src/public_assets.rs` |
| Dependency policy | Shipped gate | Advisory, license, source, and duplicate checks are owned by `cargo xtask audit`; unsafe Rust is forbidden. | `deny.toml`, `Cargo.toml`, `src/lib.rs` |
| Source and architecture policy | Shipped gate | First-party source files are limited to 500 lines, complexity is bounded, ignored artifacts are excluded, and inward dependency rules are mechanically checked. | `xtask/src/source_limits.rs`, `xtask/src/policy.rs` |
| GNU/Linux compatibility | Candidate gate | The x86-64 GNU archive is built on Ubuntu 22.04, rejects required symbols newer than `GLIBC_2.35`, and starts from the final archive on Ubuntu 22.04, Debian bookworm, and Ubuntu 24.04 before promotion. | `xtask/src/linux_compat.rs`, `.github/workflows/release-candidate.yml` |

## Public release verification

The final visual pass now has 15 reviewed complete-screen snapshots. They cover
the board, command and help overlays, both session-browser layouts, update
prompts at wide, narrow, and shallow sizes, themes, durability, attachments,
drag state, and four-direction agent controls. The snapshot expansion caught
and fixed shallow-overlay footer collision and narrow browser-footer overflow.

The complete local release gate passes on Apple silicon macOS: the canonical
quality gate, 13 PTY scenarios, the coverage floor, dependency policy, the
isolated installed-product contract, Rust 1.88, the exact three-target release
plan, and the non-publishing release rehearsal. Actionlint passes. Zizmor
reports no findings in offline mode. The generated formula passes Homebrew
style inspection under its real `Formula/proqi.rb` name.

Hosted CI passes on Linux and macOS after the repository became public. The
exact tagged release commit passed its CI and non-publishing release rehearsal.
The public release contains the expected archives, checksums, SPDX SBOMs,
attestations, generated formula, and reviewed notes.

The public `oborchers/homebrew-tap` formula passes Homebrew style, strict audit,
all-platform parsing, reinstall, and `brew test`. Installed release binaries
return the current versioned capability envelope. The tap's hosted Linux and
macOS workflow also passes.

Repository metadata, topics, private vulnerability reporting, stable-tag
protection, and `main` protection are active. Both rulesets give Oliver's
individual GitHub user an `always` bypass. The release workflow publishes after
an allowed stable tag is created, without a redundant environment approval.

The README, CLI help, Cargo metadata, release notes, and skill have been checked
against this matrix. Optional aggregate adoption reporting remains deliberately
unimplemented and is not a release blocker.
