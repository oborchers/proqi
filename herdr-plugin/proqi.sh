#!/bin/sh
# Launcher shared by the Proqi Herdr plugin action and its pane entrypoint.
#
# It only locates an installed proqi and replaces itself with it. It never
# installs, replaces, or shadows Proqi, and it never interprets Herdr state:
# `proqi herdr toggle` owns every decision.
set -eu

notify() {
  printf 'proqi herdr plugin: %s\n' "$1" >&2
  "${HERDR_BIN_PATH:-herdr}" notification show Proqi --body "$1" >/dev/null 2>&1 || true
}

# An existing installation on PATH always wins. The standalone installer's
# default directory is the only fallback because it may be missing from PATH.
resolve_proqi() {
  if found=$(command -v proqi 2>/dev/null) && [ -n "$found" ]; then
    printf '%s\n' "$found"
    return 0
  fi
  standalone="${PROQI_INSTALL_DIR:-${HOME:-}/.local/bin}/proqi"
  if [ -f "$standalone" ] && [ -x "$standalone" ]; then
    printf '%s\n' "$standalone"
    return 0
  fi
  return 1
}

if ! proqi=$(resolve_proqi); then
  notify 'Proqi is not installed. Reinstall the plugin with: herdr plugin install oborchers/proqi'
  exit 1
fi

case "${1:-}" in
  toggle)
    capabilities=$("$proqi" --json capabilities 2>/dev/null) || capabilities=
    case "$capabilities" in
      *'"herdr_companion_toggle":true'*) ;;
      *)
        printf 'proqi herdr plugin: %s does not provide proqi herdr toggle\n' "$proqi" >&2
        notify 'The installed Proqi is too old for the Herdr plugin. Update Proqi through the channel you installed it with.'
        exit 1
        ;;
    esac
    exec "$proqi" herdr toggle
    ;;
  board)
    if [ -z "${PROQI_HERDR_SESSION:-}" ]; then
      notify 'This Proqi pane was opened without a session. Use the Toggle Proqi action.'
      exit 1
    fi
    exec "$proqi" --resume "$PROQI_HERDR_SESSION"
    ;;
  *)
    printf 'usage: proqi.sh toggle|board\n' >&2
    exit 2
    ;;
esac
