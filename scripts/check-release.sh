#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Used by the Release workflow (and handy to run yourself before tagging):
#
#   sh scripts/check-release.sh v0.1.0-beta.8
#
# Checks that the tag, Cargo.toml and the top entry of CHANGELOG.md all name
# the same version. While download/ still serves older installs, it also
# checks the file there when download/version.txt lists this version.
set -eu

TAG="${1:?usage: scripts/check-release.sh <tag, like v0.1.0-beta.8>}"
V="${TAG#v}"
cd "$(dirname "$0")/.." || exit 1
fail() {
    echo "::error::$*" >&2
    echo "Release check failed: $*" >&2
    exit 1
}

CARGO="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)"
[ "$V" = "$CARGO" ] || fail "the tag is v$V but Cargo.toml says $CARGO"

TOP="$(sed -n 's/^## v\([^ ]*\).*/\1/p' CHANGELOG.md | head -n 1)"
[ "$V" = "$TOP" ] || fail "the tag is v$V but the top of CHANGELOG.md is v${TOP:-nothing}"

case "$V" in
    *-*) CH="beta"; DIR="download/beta" ;;
    *) CH="stable"; DIR="download" ;;
esac
if [ -f download/version.txt ]; then
    LISTED="$(sed -n "s/^${CH}_version=//p" download/version.txt | head -n 1 | tr -d '\r')"
    if [ "${LISTED#v}" = "$V" ]; then
        SUM="$(sed -n "s/^${CH}_sha256=//p" download/version.txt | head -n 1 | tr -d '\r')"
        [ -f "$DIR/stream-preflight-linux.gz" ] || fail "$DIR/stream-preflight-linux.gz is missing"
        GOT="$(sha256sum "$DIR/stream-preflight-linux.gz" | awk '{print $1}')"
        [ "$GOT" = "$SUM" ] || fail "$DIR/stream-preflight-linux.gz doesn't match the checksum in download/version.txt"
        echo "download/ (for older installs) also has v$V, and its checksum matches."
    else
        echo "download/ stays at ${LISTED:-nothing} for older installs (not part of this release)."
    fi
fi

echo "OK: v$V · the tag, Cargo.toml and CHANGELOG.md agree"
