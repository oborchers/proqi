# Versioned keymap and terminal delivery contract

Shortcut schema version 1 configures every semantic key and fallback through one
resolved registry. Defaults retain the foundation behavior except that Browser
management now uses F2 (Rename) and F8 (Trash), reserving uppercase R and D for
search text. Recovery Escape is explicitly modeled as a safe no-op when there
is no cancellable overlay; it never discards failed persistence.

## Input pipeline and ownership

The real input pipeline is the physical keyboard and any operating-system
mapping, terminal interception or rewriting, PTY transport, Crossterm decoding,
logical `KeyStroke`, active context, registry action, and established UI or
application intention. Proqi begins at Crossterm decoding. It never infers a
key from a physical label, test harness name, or injected text.

`KeyStroke` represents character and named keys, press, repeat, and release,
keypad and lock state, and logical Shift, Control, Alt or Option, Super, Meta,
and Hyper independently. `Primary` is not stored as a modifier. The registry
expands it to Super or Meta on macOS and Control on Linux and Windows. Raw
Control and Option are not additional Primary modifiers on macOS.

Literal character insertion, IME-committed text, bracketed paste payloads,
mouse actions, resize, host focus, timers, and effect completions are not
shortcut commands. They retain their existing owners.

## Active contexts

The context stack is ordered from underlying surface to active owner. Only its
last item dispatches a stroke.

| Context | Current owner | Text reservation |
| --- | --- | --- |
| Board | Whole-thought board | Plain printable characters may be commands |
| Compose | Transient thought editor | All ordinary and shifted printable text |
| Edit | Durable thought editor | All ordinary and shifted printable text |
| Help | Contextual Help overlay | None, modal navigation wins |
| Commands | Searchable Commands overlay | Query text |
| Search | Thought search overlay | Query text |
| Invocation | Editor-backed invocation completion | Editor text |
| InvocationQuery | Explicit invocation search | Query text |
| Transfer | Cross-session transfer chooser | Query text |
| GlobalDeliveryQuery | Global-delivery agent query | Query text |
| GlobalDeliveryDisposition | Global-delivery completion choice | None, modal navigation wins |
| Browser | Empty session-browser query | All query text; F2/F8 management while empty |
| BrowserQuery | Nonempty session-browser query | Query text |
| Rename | Board session-name editor | Name text |
| BrowserRename | Session Browser name editor | Name text |
| Update | Update choice | None, modal navigation wins |
| Screenshot | Screenshot takeover and quit choice | None, modal navigation wins |
| Recovery | Failed-durability recovery | None, recovery routes remain reachable |
| Direction | Adjacent-agent direction chooser | None, four-way navigation wins |
| ReleaseHighlights | Scrollable release highlights | None, modal navigation wins |
| InsertionBoundary | Board insertion row | Board commands, with thought-only range and reorder no-ops |

Every context whose table row reserves editor, query, or name text rejects a
plain, shifted, Option/Alt, or Control+Alt (AltGr-compatible) printable binding.
Crossterm cannot reliably distinguish AltGr-produced text from a deliberate
Control+Alt character chord, so the latter is reserved too. Literal insertion,
IME commits and bracketed paste never re-enter shortcut lookup. Named Alt or
Control+Alt keys remain independently configurable. Browser management uses
F2 and F8 only while its query is empty; R and D are always query text.

ScreenshotCommitBarrier and UpdateBarrier are typed routing barriers, not
shortcut contexts. The screenshot barrier defers the original `KeyStroke`
before resolution and resolves it after the commit finishes. The accepted
update barrier drops keyboard input before resolution. Neither barrier owns a
semantic binding or participates in collision precedence.

## Registry descriptor inventory

Every semantic action has one stable `ShortcutActionId`. Its descriptor owns:

- active contexts;
- effective macOS and portable defaults;
- context-qualified compatible and configuration aliases;
- ordinary, text-editing, destructive, invariant-close, or recovery-critical
  safety classification;
- ordered Help visibility and labels, plus footer copy and measurement policy;
- Commands visibility, availability, ordering, and the existing visible label;
- one content-free diagnostics identity;
- the mapping into an established typed UI intention or application action.

The closed action inventory covers all 53 current Commands actions plus direct
close, confirm, text editing, navigation, selection, clipboard, history,
submission, Board, Browser management, recovery, and direction actions. The
source of truth is `ShortcutActionId::COMMANDS` plus the registry's
`DIRECT_ACTIONS`; parity tests reject missing descriptors, duplicate identities,
stale Help, footer, or Commands references, and missing diagnostics.

## Configuration schema version 1

Place the following in the existing platform-native `config.toml`:

```toml
[keymap]
schema_version = 1

[keymap.bindings.board]
"submission.submit_remove" = [
  { key = "Enter", modifiers = ["Primary"] },
  { key = "s" },
  { key = "F5" },
]
"submission.submit_keep" = []
"thought.delete" = [{ key = "Delete" }, { key = "d" }]

[keymap.macos.edit]
"submission.submit_remove" = [{ key = "Enter", modifiers = ["Super", "Alt"] }]

[keymap.portable.edit]
"submission.submit_remove" = [{ key = "Enter", modifiers = ["Control", "Shift"] }]
"submission.submit_keep" = [] # removes the default collision on this chord
```

`bindings` applies to both policies; `macos` and `portable` replace the common
list for the same context/action pair. Every supplied list replaces **all**
factory aliases for that pair, including Primary and Board fallback spellings.
Omitted pairs inherit defaults. `[]` explicitly disables keyboard access where
safe; mouse and Commands actions retain their semantic identities. There is no
implicit append, case folding, expression language, chord sequence, or hidden
last-wins conflict resolution. All custom characters and modifiers match the
logical event exactly. For example, add both `h` and `H` explicitly when a host
can report an uppercase codepoint without Shift. Existing factory and legacy
aliases retain the foundation's uppercase compatibility.

Context identifiers are the snake_case spellings in the registry, including
`insertion_boundary`, `invocation_query`, `browser_rename`, and both
`global_delivery_*` owners. See [the complete action inventory](KEYMAP_ACTIONS.md)
for stable action identifiers and eligible contexts. Actions such as
`submission.submit_to_agent` may be bound without acquiring a factory shortcut.

An alias contains one `key` and an optional list of exact modifiers. Modifiers
are `Control`, `Alt`, `Shift`, `Super`, `Meta`, `Hyper`, or policy shorthand
`Primary`. Primary expands independently to Super and Meta on macOS, and Control
on Linux/Windows. Combining Primary with Control, Super or Meta is rejected;
spell the exact modifiers instead. Duplicate modifiers and duplicate expanded
aliases are errors. Each action/context list has at most 128 aliases, within
the existing 64 KiB config limit. Both platform graphs validate before startup.

Keys are one non-control Unicode scalar, `Space`, `Escape`, `Enter`, `Backspace`,
`Delete`, `Insert`, `Tab`, `BackTab`, `Left`, `Right`, `Up`, `Down`, `Home`, `End`,
`PageUp`, `PageDown`, F1 through F35, `Null`, `CapsLock`, `ScrollLock`, `NumLock`,
`PrintScreen`, `Pause`, `Menu`, `KeypadBegin`, or the `Media.*` and `Modifier.*`
names in the registry's logical vocabulary. Keypad and lock state are retained
for inspection but do not independently change an action's binding. Press and
repeat dispatch identically; release never executes a command.

Multiple aliases have stable labels generated once from the resolved graph.
Help shows the available aliases; compact footers choose a shortest spelling.
Compatible redundant modifier spellings are omitted from labels, and paired
macOS Super/Meta spellings use the conventional Cmd label. A remaining exact
Meta-only alias is labeled Meta. An unbound mouse action has no keyboard label.
Measurement and hit testing use the exact same projection as rendering.

## Legacy translation

`[keybindings]` remains accepted as a one-time input translation, including its
historical `send`, `submit`, `move_up`, and `move_down` aliases and established
fallback-shadowing rules. No legacy character map is retained in runtime
settings. Supplying `[keybindings]` together with `[keymap]` is rejected as
ambiguous. Translate an action's entire alias list when migrating; a remapped
versioned list also removes its default Primary and named-key aliases.
Legacy Delete remapping still changes only its character spelling. In version
1, physical Delete is an ordinary configurable alias and may be removed.
Browser R/D are the explicit text-safety migration for all configurations.
No user configuration file is rewritten automatically.

## Validation and recovery

Before terminal setup, the registry rejects duplicate effective bindings in one
context, duplicate contexts, ineligible modifiers, text theft, loss or
shadowing of invariant Escape, unreachable recovery actions, stale
presentation references, and missing or duplicate action and diagnostics
identities. Overlay overlap is valid because only the top context is active.

The recovery-critical set is global durable quit, retry failed storage, and
export recovery. Their effective Recovery-context bindings are validated after
platform and configuration expansion. Board Commands and Quit must also retain
at least one binding; editors and overlays retain Escape back to their owner.
Recovery Help and footer guidance use the Recovery map, including remapped
retry and export labels. Escape never bypasses the persistence exit barrier.

## Interactive key capture

```sh
proqi diagnostics keypress --context board,edit --timeout-ms 5000
proqi --json diagnostics keypress --context help --defaults
```

The diagnostic owns raw mode for one decoded key event, with a 100 to 60000 ms
bound (5000 ms by default). Escape cancels regardless of the configured keymap.
`--defaults` bypasses all config loading, allowing capture even when normal
startup rejects the configuration. Without it, configuration errors occur
before terminal ownership. The context stack is explicitly selected for this
diagnostic; it does not inspect a different running application or execute an
action. Its top owner uses the same resolver as the TUI.

Capture JSON has `capture_schema_version = 1`, `status`, `context_source`,
`keymap_source`, selected `context_stack`, `timeout_ms`, `explanation`, and an
optional `event`. A received event includes the logical key (characters as
Unicode codepoints), exact modifiers, phase, relevant state, platform policy,
context stack/top owner, classification, stable action/binding identity, and
UI intention. Reserved text and unbound input have no executed action. Arbitrary
terminal responses and paste contents are never included.

Statuses distinguish `event_received`, `cancelled`, and `no_event_received`.
The last means no key event reached this capture before its deadline. Proqi
cannot determine whether Ghostty, the OS, Karabiner, Herdr or another layer
consumed a chord. Terminal state is restored on completion, cancellation,
timeout, errors and unwinding; no discovery runs on the reducer thread.

Ghostty can emit CSI-u with an explicit `csi:` action. For Smart Paste the
existing example is `keybind = super+shift+v=csi:118;10u`, which emits
`ESC [ 118 ; 10 u` (logical v, Shift+Super). The payload is covered through the
real macOS PTY/Crossterm decoder. Alternate logical keys, layouts, host mappings
and protocol support can produce different results. Check the event delivered
in the intended pane with the diagnostic. No Proqi operation edits Karabiner,
Ghostty, Herdr, shell, OS or keyboard configuration.

Ghostty's macOS defaults consume many logical Super bindings before the PTY.
They may also rewrite Cmd+Left and Cmd+Right as raw Ctrl+A and Ctrl+E. When an
upstream remapper rewrites Home and End to those same Command arrows, the two
physical routes are indistinguishable to Herdr and Proqi. The registry never
special-cases Ctrl+A or Ctrl+E based on assumed hardware or remapper origin.
Preserving both meanings requires distinct upstream output. A user can instead
configure an exact contextual Control alias when intentionally choosing one
meaning.

For example, this deliberately treats received Ctrl+A and Ctrl+E events as
logical line boundaries in Edit while retaining the named Home and End aliases:

```toml
[keymap.macos.edit]
"editor.line_start" = [
  { key = "Home" },
  { key = "a", modifiers = ["Control"] },
]
"editor.line_end" = [
  { key = "End" },
  { key = "e", modifiers = ["Control"] },
]
```

That choice cannot distinguish two physical routes which an earlier layer has
already collapsed to the same Ctrl event. Use the capture diagnostic before
choosing the alias, and repeat the complete alias list for each additional text
context that should share it.

## Design references and licenses

The implementation was written independently. No source code or prose was
copied from these references.

- Crossterm 0.29 event types and parser behavior, MIT license:
  <https://docs.rs/crossterm/0.29.0/crossterm/event/index.html>
- Kitty keyboard protocol modifier, event-type, and CSI-u semantics, referenced
  as a protocol specification; Kitty source is GPL-3.0:
  <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>
- Visual Studio Code when-clause context model and conflict behavior, referenced
  conceptually; VS Code source is MIT and its documentation repository is
  CC-BY-4.0: <https://code.visualstudio.com/docs/configure/keybindings>
- Helix keymap context organization, MPL-2.0:
  <https://docs.helix-editor.com/keymap.html>
- Zed context keymaps, referenced conceptually; Zed source is GPL-3.0:
  <https://zed.dev/docs/key-bindings>

Proqi retains a smaller typed contract. It does not implement when-clause
expressions, user-authored context predicates, or the Kitty protocol itself.

The empty insertion row independently retains Commands and Quit aliases, because
Escape cannot expose another owner when no thought exists. Factory Primary+D
in text-entry contexts was previously dropped by the editor. It is now unbound
there, preserving its no-op behavior; an explicitly configured Duplicate alias
executes the Commands action after the pending edit is committed. Board mode
also provides `Shift+D` as a terminal-safe factory alias for Duplicate. Both
uppercase-without-Shift and lowercase-with-Shift terminal reports resolve to it.

PageUp and PageDown keep distinct ordinary and Shift-extended action identities.
On the Board they move or extend exactly five thoughts and clamp before the
insertion boundary. Compose and Edit retain their five-visual-row behavior.

The macOS factory map adds exact `Option+Shift+Up` and
`Option+Shift+Down` aliases for `thought.move_up` and `thought.move_down` in
Board and InsertionBoundary. The configured `k` and `j` vertical spellings use
the same logical modifier ladder when the terminal reports the configured
character with exact Option and Shift modifiers. A keyboard layout may instead
produce composed text, which does not impersonate that binding. These aliases
do not redefine Primary and do not apply to text owners. Compose and Edit retain
Option-based word and fast navigation. Portable Alt+Shift retains Board range
extension.

A modified uppercase-only logical codepoint is displayed explicitly, for example
`Ctrl+U+0044`, to distinguish it from the conventional `Ctrl+D` label for lowercase
`d`. Equivalent case aliases for the same action share the conventional label.

`thought.reflow` cleans spacing in exactly one existing thought. Its defaults are
plain `f` in Board and `Control+Shift+F` in Edit on every platform. Uppercase `F`
reported with Control but without a distinct Shift bit is the compatibility
spelling for terminals that encode Shift in the character. Plain Edit `f` remains text. The
complete active thought is transformed regardless of text selection. The action
has contextual Help and a Commands entry, with no permanent footer control.
Versioned aliases replace or disable these defaults using the ordinary schema.
