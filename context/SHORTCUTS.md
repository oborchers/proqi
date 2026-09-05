# Shortcut architecture and current inventory

This document records the behavior-preserving shortcut foundation. It is an
implementation contract, not a public keymap redesign. The public TOML schema,
visible labels, Help content, footer content, Commands inventory, and current
defaults remain unchanged.

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
Control is not a second Primary on macOS.

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
| Browser | Empty session-browser query | Query text, plus empty-query management aliases |
| BrowserQuery | Nonempty session-browser query | Query text |
| Rename | Board session-name editor | Name text |
| BrowserRename | Session Browser name editor | Name text |
| Update | Update choice | None, modal navigation wins |
| Screenshot | Screenshot takeover and quit choice | None, modal navigation wins |
| Recovery | Failed-durability recovery | None, recovery routes remain reachable |
| Direction | Adjacent-agent direction chooser | None, four-way navigation wins |
| ReleaseHighlights | Scrollable release highlights | None, modal navigation wins |
| InsertionBoundary | Board insertion row | Board commands, with thought-only range and reorder no-ops |

Compose, Edit, Commands, Search, Invocation, InvocationQuery, Transfer, Browser,
BrowserQuery, Rename, and BrowserRename are the discovered text fields. A plain
or shifted printable registry binding is invalid in those text-owning contexts.
Browser management `R` and `D` exist only while its query is empty.

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

The closed action inventory covers all 51 current Commands actions plus direct
close, confirm, text editing, navigation, selection, clipboard, history,
submission, Board, Browser management, recovery, and direction actions. The
source of truth is `ShortcutActionId::COMMANDS` plus the registry's
`DIRECT_ACTIONS`; parity tests reject missing descriptors, duplicate identities,
stale Help, footer, or Commands references, and missing diagnostics.

## Existing configuration projection

The existing `[keybindings]` object is parsed unchanged and translated into
registry overrides at load time. It contains:

- Board characters for new, edit, delete, copy, cut, submit and remove, submit
  and keep, undo, focus up and down, range up and down, collapse, selection,
  contextual transform, select all, range selection, search, Commands, Help,
  quit, and Screenshot Inbox;
- the lowercase exact-paste character and compatible uppercase reflow spelling;
- shifted Primary suffixes for sentence deletion and visual-row selection to
  the start or end;
- historical TOML aliases `send`, `submit`, `move_up`, and `move_down`.

Historical shadowing remains intentional compatibility: an explicit Board
command wins over the transform or paste fallback using the same character.
Uppercase reports without a distinct Shift bit retain their established
compatibility. Unmodified physical Delete remains the invariant Board delete
alias, while modified Delete and Backspace remain distinct.

## Validation and recovery

Before terminal setup, the registry rejects duplicate effective bindings in one
context, duplicate contexts, ineligible modifiers, text theft, loss or
shadowing of invariant Escape, unreachable recovery actions, stale
presentation references, and missing or duplicate action and diagnostics
identities. Overlay overlap is valid because only the top context is active.

The recovery-critical set is global durable quit, retry failed storage, and
export recovery. Their effective Recovery-context bindings are validated after
platform and configuration expansion. No new recovery UI is introduced.

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
