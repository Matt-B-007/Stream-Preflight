#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Prints the release notes for a tag: its CHANGELOG.md section, then the
# one-line install command.
#
#   sh scripts/release-notes.sh v0.1.0-beta.8
set -eu

tag="${1:?usage: scripts/release-notes.sh <tag>}"
cd "$(dirname "$0")/.." || exit 1
v="${tag#v}"

notes="$(awk -v head="## v$v" '$0 == head { f = 1; next } /^## / { if (f) exit } f' CHANGELOG.md)"
if [ -z "$notes" ]; then
    notes="Stream Preflight $tag"
fi
printf '%s\n\n' "$notes"
printf 'Install or update with one line:\n\n'
printf '    curl -fsSL https://raw.githubusercontent.com/Matt-B-007/Stream-Preflight/main/install.sh | sh\n\n'
printf 'Every download is checked against SHA256SUMS, which is signed (SHA256SUMS.minisig).\n'
