# Screenshot Inbox README demo

The Screenshot Inbox GIF is recorded from the real release binary with:

```shell
./scripts/readme-demo.sh record-inbox
```

The script creates a private disposable state directory, selects the automatic
theme, and seeds one thought with three numbered steps. The
recorder enables Screenshot Inbox with `i`, copies the checked-in Proqi logo
into the isolated watched directory as a synthetic capture, waits for the new
image thought, and annotates it in the editor.

The scene uses a 92 by 30 terminal with the repository's reference Ghostty font.
`agg` renders the cast at 1132 by 775 pixels. No user Desktop, screenshot name,
session, or machine path is recorded.
