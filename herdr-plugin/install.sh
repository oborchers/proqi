#!/bin/sh
# Build step of the Proqi Herdr plugin. Herdr shows it in the install preview.
#
# An existing Proqi installation always wins: when `proqi` is on PATH or in the
# standalone installer's directory, this step changes nothing. Otherwise it
# runs Proqi's documented standalone installer from the latest GitHub Release,
# after checking the installer against the SHA-256 record published beside it.
# Both files come from the same release, so the check protects integrity in
# transit, not authenticity; the trust anchor is GitHub over HTTPS, exactly as
# for `curl .../proqi-installer.sh | sh`. The installer then verifies the
# archive checksum and version and records a standalone installation that
# Proqi's own updater manages.
set -eu

REPOSITORY_URL='https://github.com/oborchers/proqi'
LATEST_URL="$REPOSITORY_URL/releases/latest/download"
INSTALLER='proqi-installer.sh'
MAX_INSTALLER_BYTES=1048576
MAX_CHECKSUM_BYTES=512

fail() {
  printf 'proqi herdr plugin: %s\n' "$*" >&2
  exit 1
}

if found=$(command -v proqi 2>/dev/null) && [ -n "$found" ]; then
  printf 'Using the existing Proqi at %s; nothing was installed.\n' "$found"
  exit 0
fi
# The same predicate as the launcher: only an executable file counts.
standalone="${PROQI_INSTALL_DIR:-${HOME:-}/.local/bin}/proqi"
if [ -f "$standalone" ] && [ -x "$standalone" ]; then
  printf 'Using the existing Proqi at %s; nothing was installed.\n' "$standalone"
  exit 0
fi
if [ -e "$standalone" ] || [ -L "$standalone" ]; then
  fail "$standalone exists but is not an executable file; repair or remove it, then install again"
fi

for tool in curl mktemp rm sh; do
  command -v "$tool" >/dev/null 2>&1 || fail "required command not found: $tool"
done
if command -v sha256sum >/dev/null 2>&1; then
  digest() { sha256sum "$1"; }
elif command -v shasum >/dev/null 2>&1; then
  digest() { shasum -a 256 "$1"; }
else
  fail 'sha256sum or shasum is required'
fi

temporary=$(mktemp -d "${TMPDIR:-/tmp}/proqi-herdr-plugin.XXXXXX") \
  || fail 'could not create a temporary directory'
trap 'rm -rf "$temporary"' EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

download() {
  curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
    --tlsv1.2 --connect-timeout 10 --max-time 120 --max-redirs 5 --retry 2 \
    --max-filesize "$2" --output "$temporary/$1" "$LATEST_URL/$1"
}

printf 'Proqi is not installed. Installing the latest release with its standalone installer.\n'
download "$INSTALLER.sha256" "$MAX_CHECKSUM_BYTES" \
  || fail 'could not download the installer checksum; check the network and run the install again'
download "$INSTALLER" "$MAX_INSTALLER_BYTES" \
  || fail 'could not download the installer; check the network and run the install again'

record=$(cat "$temporary/$INSTALLER.sha256") || fail 'could not read the installer checksum'
case "$record" in
  *'
'*) fail 'installer checksum must contain one exact record' ;;
esac
set -f
set -- $record
set +f
[ "$#" -eq 2 ] || fail 'installer checksum must contain one exact record'
[ "$2" = "$INSTALLER" ] || fail 'installer checksum names a different file'
case "$1" in
  ????????????????????????????????????????????????????????????????) ;;
  *) fail 'installer checksum is not SHA-256' ;;
esac
case "$1" in *[!0-9a-f]*) fail 'installer checksum is not lowercase hexadecimal' ;; esac
actual=$(digest "$temporary/$INSTALLER") || fail 'could not hash the installer'
[ "${actual%% *}" = "$1" ] || fail 'installer checksum verification failed'

sh "$temporary/$INSTALLER" --version latest
