# Proqi

Proqi is a terminal-native scratchpad for people who work with several coding
agents at once. Each process owns one resumable board of independently editable
thoughts. The project is currently a private alpha under active development.

## Prerequisites

Install Rust through `rustup`. The checked-in toolchain file selects the exact
compiler and required components. Developer quality commands additionally use
`cargo-nextest`, `cargo-llvm-cov`, `cargo-deny`, and `cargo-audit`.

Verify the local setup:

```shell
cargo xtask setup
```

## Build and run

```shell
cargo build --locked
cargo run --bin proqi
```

Running without a subcommand opens the interactive board in the current
terminal. Paste in board mode to create and focus one thought, press `Enter` to
edit the focused thought, press `Esc` to return to the board, and press `?` for
the complete contextual key guide. Changes are autosaved and can be resumed by
their canonical session identifier.

Use `y` or Primary+C to copy a complete thought, `x` or Primary+X to cut only
after clipboard success, and Primary+V to read the native clipboard. OSC 52 is
used as a write fallback when the native provider is unavailable. If autosave
fails, Proqi keeps the board in memory and blocks destructive exit. Press `r`
to retry the retained operation or `w` to atomically export a private recovery
JSON file in Proqi's platform data directory.

## CLI workflow

```shell
proqi                         # start a fresh resumable session
proqi -c                      # continue the latest inactive session here
proqi -r <id-or-name>         # resume a specific session
proqi sessions                # list sessions
proqi sessions rename <id> "name"
proqi sessions trash <id>
proqi sessions restore <id>
printf '%s' 'prompt text' | proqi thoughts add <session-id>
proqi thoughts list <session-id>
proqi thoughts inspect <session-id> <thought-id>
proqi thoughts delete <session-id> <thought-id>
proqi thoughts undo <session-id>
```

Add `--json` for the versioned machine contract. Mutations accept an optional
typed `--operation-id` and return the original durable receipt on a matching
retry, including after another process has restarted. Permanent session pruning
is separate from recoverable trash and requires `--yes`.

## Terminal configuration

Proqi reads an optional `config.toml` from the platform-native Proqi
configuration directory. The file is bounded, parsed strictly, and changed to
user-only permissions when loaded. Themes are `auto`, `light`, `dark`, or
`limited`. Direct board bindings accept one distinct printable character:

```toml
theme = "auto"

[keybindings]
new = "n"
edit = "e"
delete = "d"
copy = "y"
cut = "x"
undo = "u"
focus_up = "k"
focus_down = "j"
move_up = "K"
move_down = "J"
collapse = " "
commands = "/"
help = "?"
quit = "q"
```

Arrow keys, `Enter`, `Esc`, and primary-modifier editing shortcuts remain
portable fallbacks. Press `/` to search and run available commands.

## Development

```shell
cargo xtask format       # apply formatting
cargo xtask source-limits # enforce the 500-line source-file ceiling
cargo xtask architecture # enforce module and adapter ownership boundaries
cargo xtask check        # architecture, limits, formatting, Clippy, and tests
cargo xtask test         # deterministic test suite
cargo xtask test-pty     # pseudo-terminal scenarios
cargo xtask coverage     # write target/coverage/lcov.info
cargo xtask audit        # advisories, licenses, sources, and dependency policy
cargo xtask package      # release build plus temporary-prefix launch smoke test
```

CI invokes the same `xtask` commands. Run `cargo xtask check` before committing.
First-party source files may contain at most 500 physical lines. Rust functions
are also gated by Clippy's 80-line, nesting-depth, and cognitive-complexity
checks. Any future frontend language must add its native complexity lint before
frontend source is accepted, and it remains subject to the same file ceiling.

An optional checked-in pre-commit hook runs the same complete check. Enable it
for this clone explicitly:

```shell
cargo xtask install-hooks
```

The hook is a local convenience. CI remains authoritative, and builds never
change Git configuration automatically.

## Product and architecture

`PRODUCT.md` is the source of truth for user-visible behavior.
`ARCHITECTURE.md` defines the implementation boundaries and quality contract.

The final open-source license, Homebrew package identity, signing, notarization,
and publication setup remain deliberately undecided. Nothing in this private
version publishes artifacts or changes repository visibility.
