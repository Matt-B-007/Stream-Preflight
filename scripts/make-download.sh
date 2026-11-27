#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Puts a finished Linux build into download/ so the one-line install and the
# in-app updater can find it. Run from the repository folder:
#
#   scripts/make-download.sh <path to the stream-preflight program>
#
# The version is read from the program itself (stream-preflight --version).
# Versions with a dash (0.1.0-beta.4) go to download/beta/, others to
# download/. download/version.txt is updated and keeps the other channel.
set -eu

BIN="${1:?usage: scripts/make-download.sh <path to stream-preflight>}"
[ -x "$BIN" ] || { echo "not a program: $BIN" >&2; exit 1; }
VER="$("$BIN" --version | tr -d '\r\n')"
case "$VER" in
  *-*) CH=beta; DIR=download/beta ;;
  *) CH=stable; DIR=download ;;
esac
mkdir -p "$DIR"
gzip -9 -n -c "$BIN" > "$DIR/stream-preflight-linux.gz"
SUM="$(sha256sum "$DIR/stream-preflight-linux.gz" | awk '{print $1}')"

old() { [ -f download/version.txt ] && sed -n "s/^$1=//p" download/version.txt | head -n 1 || true; }
SV="$(old stable_version)"; SS="$(old stable_sha256)"; SN="$(old stable_notes)"
BV="$(old beta_version)"; BS="$(old beta_sha256)"; BN="$(old beta_notes)"
NOTES="${NOTES:-}"
if [ "$CH" = stable ]; then SV="$VER"; SS="$SUM"; SN="$NOTES"; else BV="$VER"; BS="$SUM"; BN="$NOTES"; fi

cat > download/version.txt <<EOF
# Stream Preflight downloads. The app and install.sh read this file.
# Written by scripts/make-download.sh; don't edit the checksums by hand.
stable_version=$SV
stable_sha256=$SS
stable_notes=$SN
beta_version=$BV
beta_sha256=$BS
beta_notes=$BN
EOF
echo "$CH v$VER -> $DIR/stream-preflight-linux.gz ($SUM)"
