#!/bin/sh
set -eu

name="$1"
shift

cleanup() {
  docker rm --force "$name" >/dev/null 2>&1 || true
}

trap cleanup EXIT
trap 'exit 130' HUP INT TERM

docker run --name "$name" --label dev.proqi.ci-linux=true "$@" &
client_pid="$!"
(
  trap '' HUP INT TERM
  while kill -0 "$client_pid" 2>/dev/null; do
    sleep 1
  done
  cleanup
) &
watchdog_pid="$!"

set +e
wait "$client_pid"
status="$?"
set -e
cleanup
wait "$watchdog_pid" || true
exit "$status"
