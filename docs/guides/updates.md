# Update Proqi and active sessions

<span class="version-scope">Proqi 0.11.0</span>

Release builds check the stable GitHub release channel at eligible interactive
startups. The request contains no thought, session, path, attachment, or agent
content. Checks are coordinated across one installation so a group of Proqi
processes does not produce a group of prompts.

Set `check_for_updates = false` in `config.toml` to disable startup checks.
Local composition, saving, and recovery remain available offline. You can still
run an explicit check from **Commands** with **Check for updates**, or from the
command line:

```sh
proqi update check
```

An explicit check reports whether the installed release is current, another
session is already checking, a skipped release is available, or a newer stable
release is available. It never installs by itself.

## Choose what happens to one release

When an eligible startup finds a newer stable release, one Proqi process owns
the installation-wide prompt. Use the arrow keys, `j` and `k`, the scroll
wheel, or a left click to choose. Press `Enter` or click the choice. `Escape`
and the close control choose **Not now**.

- **Update and restart all sessions** appears for a verified Homebrew formula
  or standalone installation. The label includes the verified number of
  affected sessions.
- **Not now** defers that exact release until the next successful eligible
  startup check.
- **Skip X.Y.Z** suppresses that exact version until a newer stable release
  exists.

These choices apply to the installation, not only the board that displayed the
prompt. Other sessions continue normally while one prompt is active.

## Install and restart from the prompt

Automatic installation is supported only when Proqi verifies that the running
executable belongs to either the `oborchers/tap/proqi` Homebrew formula or the
release-attached standalone installer on macOS or Linux.

Before changing the executable, Proqi asks every compatible live process from
that installation to finish durable work and become ready. A save failure,
timeout, lost coordinator, incompatible owner, schema activity, or Screenshot
Inbox work that is queued, saving, failed, or retryable cancels before the
installer runs. Prepared sessions return to ordinary use.

For Homebrew, Proqi runs the formula upgrade directly. For a standalone
installation, it downloads the release-attached installer and checksum over
HTTPS, verifies them, and keeps the existing verified user-owned install
directory. Neither route uses `sudo`.

After a successful installation, Proqi replaces the prepared processes and
resumes each exact session. Peers restart first and the session that initiated
the update restarts last. A completed in-app update then shows packaged
**What's new** highlights in the initiating session. The highlights need no
runtime network request and remain available from **Commands**.

## Recover from an interrupted restart

Preparation before installation is reversible. If installation fails, the old
processes remain usable and no restart is attempted.

After installation, a process that has entered irreversible restart waiting no
longer accepts ordinary mutation. If replacement cannot finish, Proqi keeps the
unfinished restart state and identifies the exact sessions that still need
attention. Follow the displayed exact resume instructions for those sessions.
Already restarted peers are not rolled back, and Proqi does not claim the
cohort is complete until every required replacement is verified.

Do not delete state files or edit the database to clear an update warning. If a
session remains blocked, run [`proqi doctor`](../reference/privacy-and-diagnostics.md#run-read-only-health-checks)
and collect a redacted diagnostic bundle before changing any user data.

## When Proqi cannot update itself

Cargo, Debian, source, development, and otherwise unverified installations do
not let Proqi invoke a package manager or rewrite the executable. An explicit
check provides the latest release location instead. Update through the same
method you used to install Proqi, then start or resume the session normally.

During the pre-1.0 series, only the latest stable release is supported. Drafts,
prereleases, malformed tags, and older or equal versions never produce an
update prompt.

See [Configuration](configuration.md#core-settings) for the startup setting,
[Privacy and diagnostics](../reference/privacy-and-diagnostics.md) for the
network and support-data boundary, and [Sessions and recovery](organization-and-recovery.md)
for ordinary durability failures.
