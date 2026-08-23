#!/bin/sh
set -eu

demo_state="${TMPDIR:-/tmp}/proqi-readme-demo-vhs-private-alpha"

clean_state() {
    if [ -L "$demo_state" ]; then
        echo "refusing symlinked demo state" >&2
        exit 1
    fi
    if [ -d "$demo_state" ]; then
        find "$demo_state" -depth -delete
    elif [ -e "$demo_state" ]; then
        echo "refusing non-directory demo state" >&2
        exit 1
    fi
}

case "${1:-}" in
    fresh)
        clean_state
        session=$(./target/release/proqi --state-dir "$demo_state" --json \
            | sed -n 's/.*"session_id":"\([^"]*\)".*/\1/p')
        test -n "$session"
        printf '%s' 'Ask the agent to preserve Unicode and exact whitespace.' \
            | ./target/release/proqi --state-dir "$demo_state" thoughts add "$session" >/dev/null
        printf '%s' 'Review the SQLite recovery path before the next release.' \
            | ./target/release/proqi --state-dir "$demo_state" thoughts add "$session" >/dev/null
        printf '%s' 'Attach the failing screenshot, then compare all four adjacent agent panes.' \
            | ./target/release/proqi --state-dir "$demo_state" thoughts add "$session" >/dev/null
        exec ./target/release/proqi --state-dir "$demo_state" -r "$session"
        ;;
    resume)
        exec ./target/release/proqi --state-dir "$demo_state" -c
        ;;
    clean)
        clean_state
        ;;
    *)
        echo "usage: $0 fresh|resume|clean" >&2
        exit 2
        ;;
esac
