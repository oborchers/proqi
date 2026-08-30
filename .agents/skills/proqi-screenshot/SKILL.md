---
name: proqi-screenshot
description: Capture or regenerate public Proqi README screenshots of real Codex and Codex-plus-Proqi Herdr panes with controlled geometry, synthetic content, privacy review, and exact asset validation.
---

# Proqi Screenshot

Create truthful, reproducible static README screenshots from real Codex, Herdr,
and Proqi interfaces. Do not redraw a harness, generate a prettier substitute,
or use private user content.

## Prepare

- Read every applicable `AGENTS.md`, `context/PRODUCT.md`,
  `context/ARCHITECTURE.md`, and `xtask/src/public_assets.rs` before changing
  public assets.
- Run from the assigned Herdr workspace and address every disposable tab and
  pane by explicit ID. Keep one implementation owner; capture panes are not
  extra implementation agents.
- Build the current topic-branch release binary. Use `theme = "auto"`; never pin
  a Proqi light or dark theme merely for the asset.
- Seed only synthetic, public-safe thoughts and conversations. Use a neutral
  visible working directory such as `~/Desktop` when a harness prints its cwd.

## Capture

1. Create a disposable Herdr tab. Add blank adjacent panes to size the actual
   target surface instead of shrinking a full-screen terminal after capture.
   At the standard 196-by-62 Herdr layout, a 98-by-31 Codex pane is a useful
   composer fixture. For the combined scene, put a 66/34 Codex-Proqi split in a
   compact upper-left group and use blank panes to control the group's outer
   width and height.
2. Start the real supported harness and the real topic-branch Proqi binary.
   Keep normal harness chrome that identifies the model, directory, promotional
   tip, composer, and status when those elements are part of the story.
3. Focus the exact tab. On Ghostty for macOS, prefer its native AppleScript
   `activate window` command plus Herdr tab focus; do not require Accessibility
   through System Events.
4. Capture into a private temporary directory with macOS `screencapture`. For
   the composer image, crop from the model panel through the input status while
   retaining the directory and promotional tip. For the combined image, crop
   the complete Codex-Proqi group. Resample once to the checked-in contract and
   remove filesystem extended attributes before staging the asset.
5. Inspect both the uncropped capture and final asset. The staged image must not
   reveal sidebars, unrelated windows, usernames, absolute paths, session IDs,
   screenshot names, private messages, credentials, or notifications.

## Verify and clean up

- Update the README, capture notes, and public-assets checks in the same change.
  Keep exact dimensions and privacy markers enforced.
- Run `git diff --check`, `cargo xtask assets`, focused script validation, and
  the repository's canonical gate. Review every generated asset diff manually.
- Close only the disposable Herdr tabs created for the capture, remove only the
  temporary state created by this run, and report the exact cleanup IDs.

Never update a harness, change macOS permissions, publish, or replace a public
asset with generated artwork unless the user explicitly authorizes that action.
