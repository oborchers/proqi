# Configurable shortcuts: discovery and design

Foundation: `75ff6851c17156953284e2adc21307e717165542` (PR #64).
Issue [#60](https://github.com/oborchers/proqi/issues/60) requests configurable
Primary submission chords. This ticket covers the complete contextual map.

## Mechanical baseline

`cargo test --lib mechanical_inventory_proves_every_claim_and_metadata_identity
-- --nocapture` emits every descriptor and effective claim, including its
platform, context, action identity, alias origin, logical stroke, and UI
intention. Every emitted claim is checked against actual registry dispatch.
Descriptor rows include safety, Help, footer, Commands, and execution metadata.
The original inventory tests also pass (9 tests).

| Foundation platform | Actions | Contexts | Expanded binding claims |
| --- | ---: | ---: | ---: |
| macOS | 98 | 21 | 13,464 |
| Portable | 98 | 21 | 13,277 |

The large claim counts include all 64 exact modifier combinations for owners
that historically ignore irrelevant modifiers. They are not distinct physical
keys. There are 88 ordinary, 2 undoable-destructive, 4 text-editing, 1 invariant
close, and 3 recovery-critical descriptors. All 52 Commands entries have
descriptors. On macOS, 26 descriptors have no binding; portable additionally
leaves the two macOS visual-row movement actions unbound.

The active-owner inventory is in `SHORTCUTS.md`. Board/editor routing lives in
`ui/app/input_dispatch.rs`; Browser, BrowserQuery, and BrowserRename routing
lives in `ui/browser/input.rs`. The screenshot commit and accepted-update
barriers run before resolution. They are not extra keymap contexts.

The audit found these migration responsibilities:

* The foundation defaults and legacy aliases are expanded by the registry, but
  Board can redispatch unresolved characters through a second legacy map.
* Help and footer still reconstruct labels from `KeyBindings`; Primary labels
  use cached default descriptors. Submission measurement passes legacy keys.
* Browser footer navigation selects specific default arrow keys explicitly.
* The keypress diagnostic waits indefinitely, drops release events, and uses a
  parallel compatibility resolver rather than a selected contextual map.
* Commands-only actions have typed execution metadata but ordinary Board/Edit
  keyboard dispatch does not execute all of them.
* Fast navigation still derives selection extension from the triggering Shift
  bit. Explicit aliases must not accidentally change the semantic action.
* Browser management historically reserves uppercase R/D in the empty query.
  New configuration must not expand this exception into arbitrary text theft.

## Professional keymap references

These are conceptual references only. No implementation source was copied.

| System | Relevant documented approach | Proqi decision |
| --- | --- | --- |
| [VS Code](https://code.visualstudio.com/docs/configure/keybindings) | Ordered contextual rules; multiple bindings; removal rules; troubleshooting reports received and dispatched events. Source MIT, documentation CC-BY-4.0. | Retain observable resolution; reject same-context collisions instead of last-rule wins. |
| [Zed](https://zed.dev/docs/key-bindings) | Context tree, platform keymaps, null disabling, key context inspector, explicit layout considerations. Source GPL-3.0; documentation consulted only. | Use closed contexts and explicit platform overrides; no expressions or copied source. |
| [Helix](https://docs.helix-editor.com/remapping.html) | TOML maps for normal/insert/select modes, named keys and explicit no-op. Source MPL-2.0; documentation consulted. | Keep editor text reservation structural and configuration local. |
| [Sublime Text](https://www.sublimetext.com/docs/key_bindings.html) | JSON key sequences, contextual predicates, platform files, Primary shorthand, noop. Proprietary product; public documentation only. | Separate exact logical modifiers from Primary policy. No physical-layout assumptions. |
| [Yazi](https://yazi-rs.github.io/docs/configuration/keymap/) | Contextual terminal maps with prepend/append and no-op rules; descriptions accompany commands. Source MIT; documentation consulted. | Derive presentation from action metadata and resolved aliases. |

These references do not establish a shared versioned schema or migration
standard. Proqi explicitly versions its own contract and rejects unknown
versions. It does not implement user predicates, macros, sequences, runtime
script execution, or a keymap plugin system.

## Terminal observations and limits

The chain is physical keyboard, Karabiner or other OS mapping, Ghostty, Herdr,
Crossterm, Proqi KeyStroke, contextual resolution. Proqi can observe only what
reaches its input decoder. No-event timeout cannot identify the consuming layer.

* [Crossterm 0.29](https://docs.rs/crossterm/0.29.0/crossterm/event/struct.KeyEvent.html)
  exposes code, exact logical modifiers, phase, and keypad/lock state. Its MIT
  source was inspected in the locally resolved Cargo dependency, including Unix
  CSI-u parsing and Windows Unicode/modifier handling.
* Its Unix alternate-key parser substitutes the shifted codepoint and clears
  Shift when such a codepoint is supplied. It does not expose the complete
  associated-text/alternate-key information of the
  [Kitty protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/).
  A diagnostic must report the decoded event, not reconstruct a physical key.
* Proqi currently requests disambiguation and alternate keys (flags 5), plus
  event types where the transport policy permits (flags 7). It does not request
  all keys or associated text. Repeat executes; release does not execute.
* Raw Control is separate from Super and Meta on macOS. Primary expands to
  either Super or Meta there, and Control on Linux/Windows. Exact combinations
  of Control, Alt, Shift, Super, Meta, and Hyper remain distinct.
* Alt/Option printable text and Control+Alt printable text can represent
  keyboard-layout or AltGr text. Text contexts must reserve those reports;
  there is no reliable physical AltGr detector in this input contract.
* Published [Herdr v0.8.2 source](https://github.com/herdrdev/herdr/tree/v0.8.2/src/input)
  is Apache-2.0. The installed server reports protocol 20. Its encoder handles
  generated text, negotiated Kitty flags, release suppression, CSI-u character
  chords and legacy named-key encodings. Pane transport uses libghostty for
  some named keys and Herdr encoding for characters. Transport receipt is not
  evidence of the resulting Proqi action.
* [Ghostty keybindings](https://ghostty.org/docs/config/keybind) run at the host
  boundary. Its [CSI action](https://ghostty.org/docs/config/keybind/reference)
  accepts the sequence without the ESC-[ prefix. `unbind` cannot remove an OS
  binding. The existing macOS Smart Paste example is
  `keybind = super+shift+v=csi:118;10u`. It spells logical Super+Shift+v, not a
  universal keyboard-layout or host guarantee. Exact-byte PTY validation and
  live topic-binary validation remain required before calling guidance tested.
* Ghostty 1.3.1 macOS defaults consume Cmd+Q, Cmd+A, Cmd+D, Cmd+J, Cmd+K,
  submission chords, vertical navigation, clipboard, and history before the
  PTY. The `performable:` prefix passes through only when Ghostty considers its
  own action unavailable, so it cannot provide general TUI fallthrough for
  actions such as quit, split, select all, or fullscreen.
* Ghostty's Cmd+Left and Cmd+Right defaults emit raw Ctrl+A and Ctrl+E. If an
  earlier remapper maps Home and End to those Command arrows, both physical
  routes have one downstream identity. Proqi cannot recover the origin. It can
  support an exact user-configured Control alias for one chosen interpretation,
  while preserving distinct named-key or CSI-u events when upstream emits them.
* [herdr-annotate at the reviewed MIT revision](https://github.com/plannotator/herdr-annotate/tree/53b6e3211a4103c3de9d361eb3f3bacc7426d23b)
  has no Ghostty integration and cannot observe physical keys. Its plugin entry
  uses configurable Herdr prefix actions. Its Bun editor inspects raw readline
  sequences for terminal compatibility aliases, while its Rust editor does not
  yet have that parity. The useful transferable rule is to resolve an explicit
  received sequence contextually, never infer an absent or pre-remapping key.

## Chosen contract

One schema version owns context/action alias lists. An action override replaces
all its defaults and fallbacks in that context. Empty lists disable safe
actions. Platform overrides replace common overrides for the same pair.
Context and action identities are closed and stable; action spellings reuse
existing diagnostics IDs. Bindings express a logical key and exact modifier
list, with Primary as an expansion shorthand.

Legacy configuration is translated once. Legacy and versioned configuration
together are rejected as ambiguous. No legacy character map remains in live
dispatch or presentation. Escape and recovery reachability are validated after
all platform and alias expansion and before terminal ownership.

The existing `diagnostics keypress` owner becomes bounded capture with explicit
context selection and a defaults-only entry that bypasses invalid configuration.
It uses the same registry resolution, reports a stable content-redacted logical
event, and restores raw mode on every exit. It cannot execute the captured
action or inspect session content. A timeout is a truthful no-event result.

Configuration and capture errors identify stable context/action/binding
positions without echoing arbitrary configuration or terminal payloads.

## Terminal-safe boundary addendum

The verified machine chain swaps physical left Control and left Command before
macOS and Ghostty. Physical `mac-symbol|alt` therefore arrives as logical
Control, while physical `opt|start` remains Option or Alt. Home and End are
rewritten upstream to Command plus Left and Right, and Ghostty may then emit the
same raw Ctrl+A and Ctrl+E used by its own Command-arrow handling. Proqi cannot
distinguish those physical sources after they collapse.

The selected defaults consequently use only bounded single logical strokes.
Exact Control plus vertical direction owns complete-thought movement in text
editors and first or last live-thought focus on the Board. On the verified macOS
input chain, diagnostics received no event for physical Option arrows, while
Control plus lowercase `n` and Control plus uppercase `N` arrived distinctly.
The macOS insertion defaults therefore follow the existing New mnemonic:
Control plus `n` inserts below, and Control plus Shift plus `n` inserts above.
The uppercase codepoint without a separate Shift bit remains a compatible
spelling for insert above. Portable platforms retain Alt plus vertical
direction because legacy terminals can collapse shifted Control letters and
portable Control remains Primary. Logical-line movement uses Control plus
horizontal direction on macOS and Alt plus horizontal direction on portable
platforms. Named Home and End remain compatible aliases. Portable configured
vertical `k` and `j` spellings join the Board insertion actions without
entering text owners. No simultaneous non-modifier chord or sequence state is
introduced.

macOS Control+Shift can extend a Board range to the boundary because it has no
prior Board owner. Portable Control+Shift remains the established
Primary+Shift reorder family, so the shifted Board boundary action is
deliberately unbound there. This preserves the existing selection and reorder
contract instead of making platform Primary ambiguous.
