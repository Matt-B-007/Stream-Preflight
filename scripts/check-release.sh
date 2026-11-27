#!/bin/sh
# Used by the Release workflow (and handy to run yourself before tagging):
#
#   scripts/check-release.sh v0.1.0-beta.5
#
# Checks that the tag, Cargo.toml and download/version.txt all name the same
# version, and that the file in download/ matches its checksum. When run in
# GitHub Actions it also passes DIR and PRE to the next steps.
set -eu

TAG="${1:?usage: scripts/check-release.sh <tag, like v0.1.0-beta.5>}"
V="${TAG#v}"
fail() { echo "::error::$*" >&2; echo "Release check failed: $*" >&2; exit 1; }

CARGO="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)"
[ "$V" = "$CARGO" ] || fail "the tag is v$V but Cargo.toml says $CARGO"

case "$V" in
  *-*) CH=beta; DIR=download/beta; PRE=true ;;
  *) CH=stable; DIR=download; PRE=false ;;
esac
[ -f download/version.txt ] || fail "download/version.txt is missing"
LISTED="$(sed -n "s/^${CH}_version=//p" download/version.txt | head -n 1 | tr -d '\r')"
SUM="$(sed -n "s/^${CH}_sha256=//p" download/version.txt | head -n 1 | tr -d '\r')"
[ "$V" = "${LISTED#v}" ] || fail "the tag is v$V but download/version.txt lists $CH ${LISTED:-nothing}"
[ -f "$DIR/stream-preflight-linux.gz" ] || fail "$DIR/stream-preflight-linux.gz is missing"
GOT="$(sha256sum "$DIR/stream-preflight-linux.gz" | awk '{print $1}')"
[ "$GOT" = "$SUM" ] || fail "$DIR/stream-preflight-linux.gz doesn't match the checksum in download/version.txt"

echo "OK: v$V ($CH) · Cargo.toml, download/version.txt and the file all match"
if [ -n "${GITHUB_ENV:-}" ]; then
  echo "DIR=$DIR" >> "$GITHUB_ENV"
  echo "PRE=$PRE" >> "$GITHUB_ENV"
fi
