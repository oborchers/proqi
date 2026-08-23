# Proqi terminal interface contract

The repository root contract applies here. This scope adds rendering and
interaction rules for the Ratatui interface.

## Rendering and layout

- Rendering is a deterministic, side-effect-free projection of application
  state. Every draw paints the complete visible viewport.
- `Frame::area()` is the source of truth for current geometry. Resize events
  wake rendering but do not define layout dimensions.
- Calculate layout and mouse hit targets together from the same rectangles.
  Never maintain an independent hit-test geometry model. Ignore mouse input
  when no hit map from the current rendered frame is available.
- Store logical editor positions. Recompute wrapped visual rows, terminal cell
  columns, cursor placement, selection geometry, and scroll bounds after each
  layout change.
- Measure terminal cells, not bytes, Unicode scalar values, or grapheme counts,
  for visual width.
- Preserve whitespace and indentation. Thought text wraps without horizontal
  scrolling. Render multiline content as structured text, never as a single
  span containing newline characters, and configure wrapping not to trim.
- Responsive degradation is explicit and tested at minimum, narrow, standard,
  wide, tall, and shallow viewport sizes.

## Input and terminal ownership

- Normalize Crossterm events before they reach application state or the
  reducer. Domain and application code never import terminal event types.
- Treat only key-press events as ordinary keyboard input. Release and repeat
  protocol events must not duplicate semantic actions.
- Read and poll Crossterm events from one input lane. Only resize notifications
  may be coalesced. Never drop or reorder keys, pastes, mouse actions, edits, or
  persistence operations. Keep event lanes bounded and preserve their ordering.
- Treat one bracketed paste as one semantic edit operation.
- Derive keyboard and mouse behavior from the same domain intentions. Keep both
  paths complete for every core action.
- Own raw mode, alternate screen, cursor visibility, mouse capture, and
  bracketed paste with an RAII guard. Restoration is best effort and must not
  panic after normal exit, setup failure, runtime error, panic, or supported
  termination signal.
- Use frame cursor APIs during rendering. Do not issue backend commands from
  ordinary widgets or render functions.
- Treat a rendering failure as a typed terminal-session failure. Restore owned
  terminal modes before returning the error.

## Verification

- Test individual widgets or render functions against in-memory buffers.
- Test complete board states with Ratatui's `TestBackend`, including cursor
  positions and repeated resize sequences.
- Cover empty, populated, editing, selection, collapsed, pending-save,
  failed-save, help, and modal states.
- Unicode cases include wide CJK, combining marks, emoji sequences,
  right-to-left samples, tabs, controls, and wrapped selections.
- Keep golden buffers explicit and review every change. Never auto-accept them.
- Retain PTY tests for real setup, escape-sequence input, resize, restoration,
  and clean shutdown. In-memory rendering tests do not replace PTY coverage.
