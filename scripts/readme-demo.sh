#!/bin/sh
set -eu

demo_state="${TMPDIR:-/tmp}/proqi-readme-demo-v1"
inbox_demo_state="${TMPDIR:-/tmp}/proqi-readme-inbox-demo-v1"
binary="./target/release/proqi"
demo_font='Meslo LG M DZ for Powerline'

clean_directory() {
    directory=$1
    if [ -L "$directory" ]; then
        echo "refusing symlinked demo state" >&2
        exit 1
    fi
    if [ -d "$directory" ]; then
        find "$directory" -depth -delete
    elif [ -e "$directory" ]; then
        echo "refusing non-directory demo state" >&2
        exit 1
    fi
}

clean_state() {
    clean_directory "$demo_state"
    clean_directory "$inbox_demo_state"
}

prepare() {
    clean_state
    umask 077
    mkdir -p "$demo_state/config"
    printf '%s\n' 'check_for_updates = false' 'theme = "auto"' \
        > "$demo_state/config/config.toml"
    cp assets/proqi-logo.png "$demo_state/demo-image.png"
    session=$("$binary" --state-dir "$demo_state" --json \
        | sed -n 's/.*"session_id":"\([^"]*\)".*/\1/p')
    test -n "$session"
    "$binary" --state-dir "$demo_state" --json sessions rename \
        "$session" "release follow-ups" >/dev/null
    printf '%s' 'Track the upstream issue at https://github.com/oborchers/proqi/issues' \
        | "$binary" --state-dir "$demo_state" thoughts add "$session" >/dev/null
    printf '%s\n' 'Release checklist:' \
        '1. Verify checksums and attestations.' \
        '2. Test the archive on a clean machine.' \
        | "$binary" --state-dir "$demo_state" thoughts add "$session" >/dev/null
}

prepare_inbox() {
    clean_directory "$inbox_demo_state"
    umask 077
    mkdir -p "$inbox_demo_state/config" "$inbox_demo_state/inbox"
    printf '%s\n' \
        'check_for_updates = false' \
        'theme = "auto"' \
        '' \
        '[screenshot_inbox]' \
        "directory = \"$inbox_demo_state/inbox\"" \
        'capture_all_new_images = true' \
        > "$inbox_demo_state/config/config.toml"
    cp assets/proqi-logo.png "$inbox_demo_state/capture-source.png"
    session=$("$binary" --state-dir "$inbox_demo_state" --json \
        | sed -n 's/.*"session_id":"\([^"]*\)".*/\1/p')
    test -n "$session"
    "$binary" --state-dir "$inbox_demo_state" --json sessions rename \
        "$session" "screenshot review" >/dev/null
}

seed() {
    expect scripts/readme-demo-seed.exp "$binary" "$demo_state"
}

render_gif() {
    cast=$1
    output=$2
    agg --quiet --theme github-dark \
        --font-family "$demo_font" --font-size 20 \
        --line-height 1.25 --fps-cap 30 --last-frame-duration 2 \
        --select '0.2..90%' \
        "$cast" "$output"
}

require_cast_success() {
    cast=$1
    if ! tail -n 1 "$cast" | grep -q '"x", "0"'; then
        echo "README demo recorder failed: $cast" >&2
        exit 1
    fi
}

record() {
    command -v asciinema >/dev/null
    command -v agg >/dev/null
    command -v expect >/dev/null
    command -v fc-match >/dev/null
    matched_font=$(fc-match -f '%{family}' "$demo_font")
    case "$matched_font" in
        "$demo_font"*) ;;
        *)
            echo "README demo requires the Ghostty font: $demo_font" >&2
            exit 1
            ;;
    esac
    prepare
    seed >/dev/null
    export COLORTERM=truecolor
    unset NO_COLOR
    asciinema record --headless --overwrite --quiet --return \
        --idle-time-limit 2 --window-size 92x30 \
        --command "expect scripts/readme-demo-record.exp $binary $demo_state" \
        target/proqi-demo.cast
    require_cast_success target/proqi-demo.cast
    render_gif target/proqi-demo.cast assets/proqi-demo.gif
}

record_inbox() {
    command -v asciinema >/dev/null
    command -v agg >/dev/null
    command -v expect >/dev/null
    command -v fc-match >/dev/null
    matched_font=$(fc-match -f '%{family}' "$demo_font")
    case "$matched_font" in
        "$demo_font"*) ;;
        *)
            echo "README demo requires the Ghostty font: $demo_font" >&2
            exit 1
            ;;
    esac
    prepare_inbox
    export COLORTERM=truecolor
    unset NO_COLOR
    asciinema record --headless --overwrite --quiet --return \
        --idle-time-limit 2 --window-size 92x30 \
        --command "expect scripts/readme-screenshot-inbox-record.exp $binary $inbox_demo_state" \
        target/proqi-screenshot-inbox.cast
    require_cast_success target/proqi-screenshot-inbox.cast
    render_gif target/proqi-screenshot-inbox.cast assets/proqi-screenshot-inbox.gif
}

case "${1:-}" in
    prepare)
        prepare
        ;;
    seed)
        seed
        ;;
    record)
        record
        ;;
    record-inbox)
        record_inbox
        ;;
    open)
        exec "$binary" --state-dir "$demo_state" -c
        ;;
    clean)
        clean_state
        ;;
    *)
        echo "usage: $0 prepare|seed|record|record-inbox|open|clean" >&2
        exit 2
        ;;
esac
