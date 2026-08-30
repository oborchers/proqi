# Screenshot Inbox README demo

The Screenshot Inbox GIF is recorded from the real release binary with:

```shell
./scripts/readme-demo.sh record-inbox
```

The script creates a private disposable state directory and selects the
automatic theme. Starting with an empty board, the recorder types four
separate thoughts live, leaves each visible long enough to read, enables
Screenshot Inbox with `i`, copies the checked-in Proqi logo into the isolated
watched directory as a synthetic capture, waits for the new image thought, and
annotates it on the same row in the editor. Two final live thoughts explain the
single-board listener and its unattended-capture and inactivity limits.

The scene uses the same 92 by 30 terminal, reference Ghostty font, automatic
palette response, and shared `agg` renderer as the primary GIF. It renders at
1132 by 775 pixels. No user Desktop, screenshot name, session, or machine path
is recorded.
