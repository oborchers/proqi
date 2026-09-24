# Complete feature index

<span class="version-scope">Proqi 0.11.0 plus next-release main</span>

<div class="feature-index" markdown>

This index is the discovery map for the published v0.11.0 product plus additive
commands already on `main`. **Shipped** works in the published binary.
**Next release** is implemented on `main` but not in the published v0.11.0
binary. **Conditional** needs the platform or verified integration named in the
row. Deliberate boundaries are included so absence is not mistaken for a hidden
setting.

The [Commands reference](commands.md) separately accounts for every searchable
action. The [CLI reference](cli.md) accounts for every public command-line
surface.

## Compose and navigate

| Feature | Availability | What to know | Learn more |
| --- | --- | --- | --- |
| Fresh and resumable boards | Shipped | Create, continue by working directory, browse, or resume by unique name or ID. | [Get started](../getting-started.md) |
| First-run practice board | Shipped | One pristine interactive install gets six ordinary persisted practice thoughts once. | [Get started](../getting-started.md#open-a-board) |
| Board and Edit contexts | Shipped | Board owns structure and groups; Edit owns one exact text body. | [Capture and edit](../guides/capture-and-edit.md) |
| Empty Compose | Shipped | Type or paste to materialize the first thought; leave with Escape for Board controls. | [Get started](../getting-started.md#learn-the-two-main-contexts) |
| Thought creation | Shipped | Create from keyboard, insertion row, mouse, paste, Commands, or repeated boundary movement. | [Capture and edit](../guides/capture-and-edit.md#create-where-you-are) |
| Directional insertion | Shipped | Insert immediately above or below focus with platform defaults or Commands. | [Commands](commands.md#thoughts-and-board) |
| Boundary and fast navigation | Shipped | Jump five items, go to first or last thought, and keep the insertion row reachable. | [Keyboard map](keyboard.md) |
| Exact Unicode editing | Shipped | Grapheme-aware movement preserves combining marks, emoji sequences, CJK, wide cells, tabs, CRLF, and whitespace. | [Capture and edit](../guides/capture-and-edit.md#edit-exact-text) |
| Wrapped-row cursor and selection | Shipped | Move or extend by rendered row edge without confusing it with a logical line. | [Keyboard map](keyboard.md#editor-essentials) |
| Mouse text selection | Shipped | Single-click placement, drag, double-click word, triple-click line, and Shift-click extension. | [Keyboard map](keyboard.md#editor-essentials) |
| Smart lists | Shipped | Continue, indent, and outdent recognized Markdown lists; insert a plain newline explicitly. | [Capture and edit](../guides/capture-and-edit.md#continue-lists-without-losing-plain-text) |
| Logical-line deletion | Shipped | Delete the exact logical line as one editor revision. | [Commands](commands.md#editing-and-transformation) |
| Unicode sentence deletion | Shipped | Delete touched Unicode sentence segments with conservative fold review and undo. | [Capture and edit](../guides/capture-and-edit.md#edit-exact-text) |
| Long-content collapse | Shipped | Persist a compact presentation without changing canonical text. | [Capture and edit](../guides/capture-and-edit.md#work-with-long-thoughts) |
| Responsive terminal layout | Shipped | Reflow narrow, wide, tall, shallow, and repeated resizes without mutating content or cursor state. | [Capture and edit](../guides/capture-and-edit.md#work-with-long-thoughts) |
| URL presentation | Shipped | Explicit HTTP and HTTPS ranges receive link styling; activation remains a terminal capability. | [Capture and edit](../guides/capture-and-edit.md) |

## Select, structure, and transform

| Feature | Availability | What to know | Learn more |
| --- | --- | --- | --- |
| Focus separate from selection | Shipped | Navigation changes focus without silently replacing an arbitrary group. | [Selection](../guides/selection.md) |
| Arbitrary multi-selection | Shipped | Toggle noncontiguous items and preserve visible Board order for bulk actions. | [Selection](../guides/selection.md#toggle-an-arbitrary-group) |
| Anchored range selection | Shipped | Extend, reverse, shrink, page, Shift-click, or use a modifier-free latch. | [Selection](../guides/selection.md#select-a-contiguous-range) |
| Select all | Shipped | Board selection and editor text selection remain context-specific. | [Selection](../guides/selection.md#toggle-an-arbitrary-group) |
| Group copy, cut, delete, duplicate, collapse, and delivery | Shipped | Each action declares how thoughts and separators participate. | [Selection](../guides/selection.md#what-group-actions-include) |
| Selected Board reorder | Shipped | Each selected run exchanges with one neighbor without wrapping; single-item keyboard movement still wraps. | [Selection](../guides/selection.md#reorder-selected-items) |
| Thought names | Shipped | Optional names organize without entering copied or delivered bodies. | [Sessions and recovery](../guides/organization-and-recovery.md) |
| Session names | Shipped | Set, clear, search, and resume by unique name. | [Sessions and recovery](../guides/organization-and-recovery.md) |
| Payload-free separators | Shipped | Persistent structural items can move and participate in history without becoming prompt content. | [Transform and organize](../guides/edit-and-transform.md#add-visual-structure-without-payload) |
| Scriptable mixed Board items | Next release | Insert, move, duplicate, or recoverably delete typed thoughts and separators as atomic Board operations. | [CLI](cli.md#change-board-items) |
| Split thought | Shipped | Divide at the exact cursor as one atomic operation. | [Transform and organize](../guides/edit-and-transform.md#split-at-the-cursor) |
| Extract selection | Shipped | Move exact selected text into a neighboring thought atomically. | [Transform and organize](../guides/edit-and-transform.md#extract-selected-text) |
| Merge thoughts | Shipped | Join a contiguous thought range with the configured exact separator. | [Transform and organize](../guides/edit-and-transform.md#merge-a-contiguous-range) |
| Existing-thought cleanup | Shipped | Clean prose spacing while preserving protected structures and annotations. | [Transform and organize](../guides/edit-and-transform.md#clean-existing-spacing) |
| Scriptable exact transformations | Next release | Split, extract, merge, and reflow with content digests, UTF-8 byte boundaries, and idempotent retry identities. | [CLI](cli.md#inspect-and-change-thoughts) |
| Persistent contextual undo and redo | Shipped | Separate Board, editor, and Browser histories survive restart and never claim to recall external effects. | [Undo and redo](undo-redo.md) |

## Paste, files, and screenshots

| Feature | Availability | What to know | Learn more |
| --- | --- | --- | --- |
| Exact bracketed and native paste | Shipped | Every paste is one exact undo unit. | [Paste and attachments](../guides/paste-and-attachments.md#paste-exact-content) |
| Explicit paste cleanup | Shipped | Cleanup is opt-in and preserves authored lines, structured content, and protected ranges. | [Paste and attachments](../guides/paste-and-attachments.md#paste-and-clean-up-spacing) |
| Large-paste folding | Shipped | Large content renders compactly while canonical text stays intact. | [Paste and attachments](../guides/paste-and-attachments.md#drop-files-and-paste-images) |
| File drop and path recognition | Shipped | Absolute, quoted, escaped, file-URL, and multi-file paths become annotations only after every path resolves. | [Paste and attachments](../guides/paste-and-attachments.md#drop-files-and-paste-images) |
| Clipboard image materialization | Shipped | Native pixels become a private session PNG and an atomic image annotation. | [Paste and attachments](../guides/paste-and-attachments.md#drop-files-and-paste-images) |
| Attachment accessibility refresh | Shipped | Recheck paths without requesting an iCloud download. | [Paste and attachments](../guides/paste-and-attachments.md#deliver-attachments-safely) |
| Native clipboard with OSC 52 fallback | Conditional | Unannotated text prefers the native clipboard; bounded OSC 52 is a terminal-dependent fallback. Annotated copy and cut require macOS typed clipboard support. | [Agent delivery](../guides/agent-delivery.md#choose-copy-or-direct-delivery) |
| Screenshot Inbox | macOS | Watch new screenshots without taking, uploading, analyzing, or reconfiguring them. | [Paste and attachments](../guides/paste-and-attachments.md#screenshot-inbox-on-macos) |
| Screenshot pause, takeover, and retry | macOS | One owner, bounded unattended capture, verified takeover, and explicit failed-capture retry. | [Paste and attachments](../guides/paste-and-attachments.md#screenshot-inbox-on-macos) |

## Discover and deliver

| Feature | Availability | What to know | Learn more |
| --- | --- | --- | --- |
| Searchable Commands | Shipped | Context-aware Unicode search across every searchable Commands entry. | [Commands](commands.md) |
| Contextual Help | Shipped | Shows effective configured bindings for the active owner. | [Discovery](../guides/discovery-and-invocations.md#use-contextual-help) |
| Thought search | Shipped | Search prompt bodies without changing Board content. | [Capture and edit](../guides/capture-and-edit.md#search-without-changing-the-board) |
| Fuzzy invocation lookup | Shipped | Discover `$`, `/`, and supported `@` namespaces with deterministic ranking. | [Discovery](../guides/discovery-and-invocations.md#complete-a-local-invocation) |
| Shared Codex and Claude built-ins | Conditional on a verified adjacent target | Insert the checked-in 19-token catalog at byte zero; the receiving harness controls behavior and availability. | [Discovery](../guides/discovery-and-invocations.md#complete-a-local-invocation) |
| Invocation refresh | Shipped | Rescan supported local roots after definitions or packages change. | [Discovery](../guides/discovery-and-invocations.md#refresh-discoveries) |
| Generic copy workflow | Shipped | Copy exact bodies into any application without requiring Herdr. | [Agent delivery](../guides/agent-delivery.md#choose-copy-or-direct-delivery) |
| Adjacent agent discovery | Conditional on Herdr | Independently verify eligible coding agents in four directions. | [Agent delivery](../guides/agent-delivery.md#deliver-to-an-adjacent-agent) |
| Global agent discovery | Conditional on Herdr | Search verified agents on the current Herdr server across tabs and workspaces. | [Agent delivery](../guides/agent-delivery.md#deliver-elsewhere-on-the-current-herdr-server) |
| Submit and keep | Conditional on Herdr | Deliver one ordered prompt and retain every source. | [Agent delivery](../guides/agent-delivery.md#understand-remove-and-keep) |
| Submit and remove | Conditional on Herdr | Remove unchanged sources only after a matching accepted receipt is durably journaled. | [Agent delivery](../guides/agent-delivery.md#understand-remove-and-keep) |
| Submit complete board | Conditional on Herdr | Deliver all live thought bodies in canonical order, keeping or removing together. | [Agent delivery](../guides/agent-delivery.md) |
| Cross-session transfer | Shipped | Copy selected thoughts as one durable destination cohort, optionally removing all sources after acceptance. | [Commands](commands.md#delivery-and-transfer) |
| Pane presentation identity | Conditional on Herdr | Advertise bounded display-only Proqi identity without impersonating an agent. | [Agent delivery](../guides/agent-delivery.md) |
| Conversation reading or response waiting | Not shipped | Proqi deliberately does neither and never falls back to raw key injection. | [Agent delivery](../guides/agent-delivery.md#choose-copy-or-direct-delivery) |

## Sessions, persistence, and recovery

| Feature | Availability | What to know | Learn more |
| --- | --- | --- | --- |
| Searchable Session Browser | Shipped | Search name, launch path, and thought content; see active, resumable, recovered, and trashed states. | [Sessions and recovery](../guides/organization-and-recovery.md) |
| Copy session identity or resume command | Shipped | Copy the complete ID or exact resume command without durable mutation. | [Commands](commands.md#sessions) |
| Session trash and restore | Shipped | Trash is recoverable and participates in persistent Browser history. | [Sessions and recovery](../guides/organization-and-recovery.md) |
| Permanent prune | Shipped CLI | Requires an already trashed session and explicit `--yes`; cannot be undone. | [CLI](cli.md#manage-sessions) |
| Multiple active sessions | Shipped | Different sessions may run together; one lease prevents dual editing of the same session. | [Sessions and recovery](../guides/organization-and-recovery.md) |
| Active-session CLI forwarding | Shipped on macOS and Linux | Reads synchronize and supported mutations travel through the active owner's reducer. Next-release main makes the unavailable result explicit in `capabilities` on other platforms without bypassing the lease boundary. | [CLI](cli.md#inspect-and-change-thoughts) |
| Autosave acknowledgement | Shipped | Footer state follows real persistence acknowledgement, not optimistic UI state. | [Sessions and recovery](../guides/organization-and-recovery.md#recover-from-a-storage-failure) |
| Storage retry and recovery export | Shipped | Retry failed durability or export optimistic state to a new private file. | [Sessions and recovery](../guides/organization-and-recovery.md#recover-from-a-storage-failure) |
| Crash recovery | Shipped | Acknowledged work survives; uncommitted transactions roll back; leases release on death. | [Privacy and diagnostics](privacy-and-diagnostics.md#read-durability-truthfully) |
| Input-stall continuity | macOS and Linux | One bounded same-pane replacement attempt resumes the exact session; otherwise prints a manual resume path. | [Sessions and recovery](../guides/organization-and-recovery.md) |

## Configuration, updates, and installed product

| Feature | Availability | What to know | Learn more |
| --- | --- | --- | --- |
| Auto, light, dark, limited, and custom themes | Shipped | Semantic colors are validated; focus never relies on color alone. | [Configuration](../guides/configuration.md#theme-safely) |
| Comfortable and compact density | Shipped | Shallow panes automatically resolve to compact spacing. | [Configuration](../guides/configuration.md#core-settings) |
| Mouse capture toggle | Shipped | Disable when a terminal or multiplexer mishandles SGR mouse reporting. | [Configuration](../guides/configuration.md#core-settings) |
| Optional footer chrome visibility | Next release | Start hidden from configuration or temporarily toggle Board for one running process; operational and recovery state stays visible. | [Configuration](../guides/configuration.md#hide-optional-footer-chrome) |
| Versioned keymap with platform overrides | Shipped | Replace exact action/context alias lists with startup validation and safe invariants. | [Shortcut troubleshooting](../guides/shortcuts.md#remap-one-action) |
| Keypress inspector | Shipped | Observe one content-redacted logical event and its resolved action. | [Shortcut troubleshooting](../guides/shortcuts.md#see-what-proqi-actually-receives) |
| Explicit update check | Shipped | Query the verified stable channel without installing. | [CLI](cli.md#check-updates) |
| Installed release highlights | Shipped | Open **What's new** for the version already running, without changing it. | [Commands](commands.md#invocations-updates-and-recovery) |
| Content-free startup update check | Shipped release builds | Coalesced across concurrent starts and individually disableable. | [Privacy and diagnostics](privacy-and-diagnostics.md#know-what-can-leave-the-machine) |
| Coordinated Homebrew update | Homebrew installs | Saves participants, runs one direct formula upgrade after confirmation, verifies, and resumes. | [Updates and restarts](../guides/updates.md#install-and-restart-from-the-prompt) |
| Coordinated standalone update | Standalone installer | Verifies the release-bound installer and checksum before replacing the existing user-owned prefix. | [Updates and restarts](../guides/updates.md#install-and-restart-from-the-prompt) |
| Non-mutating update notice | Cargo, Debian, source, and unknown installs | Update discovery does not rewrite these installation types. | [Updates and restarts](../guides/updates.md#when-proqi-cannot-update-itself) |
| Bash, Fish, and Zsh completions | Shipped | Generate from the binary; release archives also include them. | [CLI](cli.md#generate-shell-completions) |
| macOS and Linux release targets | Shipped | Native Apple, GNU glibc, static musl fallback, and Debian artifacts are published as applicable. | [Get started](../getting-started.md#install) |

## CLI, privacy, and operational safety

| Feature | Availability | What to know | Learn more |
| --- | --- | --- | --- |
| Versioned JSON capability discovery | Shipped | Discover schema, bounds, identifiers, transfer, updates, control, and optional integrations. | [CLI](cli.md#discover-capabilities) |
| Human and JSON session commands | Shipped | List, search, rename, trash, restore, history, and prune. | [CLI](cli.md#manage-sessions) |
| Atomic named sessions | Next release | `sessions ensure` returns or atomically creates the one live session with a name and directory; `sessions create` adds another named session. Neither opens a TUI. | [CLI](cli.md#create-named-sessions) |
| Named thought creation | Next release | `thoughts add --name` creates content, name, and position as one undoable operation. | [CLI](cli.md#inspect-and-change-thoughts) |
| Retry-safe session changes | Next release | Session rename, trash, restore, undo, redo, prune, and create accept `--operation-id`; repeated trash is a successful no-op. | [CLI](cli.md#retry-session-changes) |
| Bounded lists | Next release | `thoughts list` and `sessions list` accept `--limit` and `--after` and report `total` and `next_after`. | [CLI](cli.md#bounded-lists) |
| JSON help and version | Next release | `--json --help` and `--json --version` succeed with structured data and exit 0. | [CLI](cli.md#common-options) |
| Documented error codes | Next release | Every JSON error code, exit status, retry class, and `details` shape, also published in `capabilities`. | [CLI](cli.md#errors) |
| Human and JSON Board-item commands | Next release | Insert separators and move, duplicate, or delete typed thoughts and separators. | [CLI](cli.md#change-board-items) |
| Human and JSON thought commands | Shipped plus next release | The published binary provides list, inspect, add, rename, replace, collapse, move, send, delete, undo, and redo. Split, extract, merge, and reflow are next-release additions. | [CLI](cli.md#inspect-and-change-thoughts) |
| Typed canonical identifiers | Shipped | Prefixes identify resource kinds and retain complete UUIDv7 values. | [CLI](cli.md) |
| Idempotent mutations | Shipped | Matching operation identities return the original receipt; divergent semantic reuse is rejected. | [CLI](cli.md#inspect-and-change-thoughts) |
| Machine-readable errors | Shipped | Versioned envelopes use stable current error codes and nonzero exits. | [CLI](cli.md) |
| Dedicated Proqi and debug skills | Shipped | Teach compatible coding agents to use the explicit JSON CLI or investigate read-only-first without scraping storage or the TUI. Install with `npx skills add` or, in Claude Code, as the `proqi@proqi` marketplace plugin. | [CLI](cli.md#install-the-shipped-agent-skills) |
| Read-only doctor | Shipped | Checks local health without repair or mutation. | [Privacy and diagnostics](privacy-and-diagnostics.md#run-read-only-health-checks) |
| Content-redacted diagnostics | Shipped | Bounded local collection, no upload, and no overwrite. | [Privacy and diagnostics](privacy-and-diagnostics.md#collect-a-private-support-bundle) |
| User-only local state | Shipped | Private state paths, databases, companions, backups, and recovery destinations are preflighted. | [Privacy and diagnostics](privacy-and-diagnostics.md) |
| No telemetry or cloud sync | Shipped boundary | User content remains local unless a named copy or delivery action sends it elsewhere. | [Privacy and diagnostics](privacy-and-diagnostics.md) |

</div>
