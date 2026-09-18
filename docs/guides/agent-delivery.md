# Deliver prompts to agents

> Applies to Proqi 0.11.0.

Clipboard transfer does not require Herdr, but it still depends on a native
clipboard or a terminal that accepts Proqi's bounded OSC 52 fallback. Direct
agent delivery is an optional Herdr enhancement and appears only when Proqi can
verify a suitable target.

## Choose copy or direct delivery

**Copy** writes exact thought bodies through the native plain-text clipboard
when available, with bounded OSC 52 as a terminal-dependent fallback. Proqi
does not know whether another application later accepts the clipboard content.

**Direct delivery** calls Herdr's semantic agent prompt operation. An accepted
receipt means the matching harness accepted the text entry. It does not mean
the agent completed the work, produced a response, or even kept the input as a
separate turn.

Proqi never delivers by invoking a shell or injecting raw keys. It never reads
the agent conversation and does not wait for a response.

## Deliver to an adjacent agent

In a Herdr-managed pane, Proqi verifies adjacent targets above, below, left, and
right. A target must be another pane in the same tab and workspace, overlap the
requested edge, contain a supported interactive agent, and expose sufficient
identity for safe receipt matching.

With one eligible target:

- Press Board `s`, `Primary+Enter`, or macOS `Ctrl+Enter` to deliver and remove
  only after acceptance.
- Press Board `Shift+S`, `Primary+Shift+Enter`, or macOS
  `Ctrl+Shift+Enter` to deliver and keep.
- Click the visible Submit or Submit & keep control for the same action.

While editing, the Enter chords address only the active durable thought. Plain
`Enter` remains newline or smart-list continuation.

With several eligible adjacent targets, Proqi opens a direction chooser. Use an
arrow or `h`, `j`, `k`, or `l`, or click the intended adjacent target. `Escape`
cancels without changing the source.

## Deliver elsewhere on the current Herdr server

Open Commands and choose **Submit to agent...**. Search verified agents in
other tabs and workspaces on the current Herdr server. Choose a target, then
choose **Submit** or **Submit and keep**.

This route does not target arbitrary panes, named remote servers, SSH hosts,
historical sessions, or unrecognized shells. Blocked, unknown, launching, and
otherwise noninteractive agents can remain visible with delivery disabled.

## Understand remove and keep

Both dispositions send the same prompt.

- **Submit and keep** always retains every source thought.
- **Submit** stages local removal only after the exact target returns a matching
  accepted receipt and Proqi durably journals that terminal result.

The removal is one ordinary Board operation and can be undone after restart.
Undo restores only the local source. It cannot recall text already accepted by
the receiving harness.

A failed, timed-out, ambiguous, unsupported, stale, or mismatched delivery
keeps every source unchanged. If journaling or persistence fails after external
acceptance, Proqi does not pretend that removal completed and retains the local
recovery path.

## What enters the prompt

The focused thought or current thought selection is captured in visible Board
order. Bodies are joined with one blank line.

- Optional thought names are never prepended or sent as a separate field.
- Separators are payload-free and are never included.
- Stored bodies remain exact.
- For Codex and Claude Code multi-thought delivery, a complete leading `/plan`
  or `/goal` starter is retained on the first thought and omitted from later
  thought starts. In-body text and stored sources remain unchanged.
- Every annotated attachment must pass a fresh local readability check before
  Proqi creates a submission attempt.

## Know the terminal and host limits

The receiving harness decides whether input delivered while it is working
steers the current turn, queues, or is rejected. Proqi reports the observed
agent state but does not reinterpret it.

The Herdr protocols supported by Proqi 0.11.0 acknowledge accepted text entry,
but do not guarantee a distinct user-turn boundary when another sender submits
at the same time. Overlapping input can merge inside the receiving harness.
Keep deferred drafts in Proqi instead of leaving them in the harness input when
other senders may target the same agent.

Proqi also cannot prove that an external path remains present after its final
readability check and before the receiving agent opens it. Direct delivery
passes the path; it does not copy the external file.

If a delivery shortcut never reaches Proqi, use
[Configure and troubleshoot shortcuts](shortcuts.md).
