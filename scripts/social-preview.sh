#!/bin/sh
set -eu

if ! command -v rsvg-convert >/dev/null 2>&1; then
    echo "rsvg-convert is required (brew install librsvg)" >&2
    exit 1
fi

rsvg-convert \
    --width 1280 \
    --height 640 \
    assets/proqi-social-preview.svg \
    --output assets/proqi-social-preview.png
