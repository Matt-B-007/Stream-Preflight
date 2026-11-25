#!/bin/sh
# Stream Preflight installer for Linux. One line:
#
#   curl -fsSL https://raw.githubusercontent.com/Matt-B-007/Stream-Preflight/main/install.sh | sh
#
# Reads download/version.txt in the repository, downloads the newest stable
# version (or the newest beta while there's no stable one yet), checks it
# against the SHA-256 listed there, puts it in ~/.local/bin and adds it to
# your app menu. No sudo. Add "-s -- --beta" after "sh" to get the newest beta
# even when there's a stable version. After that, Stream Preflight updates itself.
set -eu

REPO="Matt-B-007/Stream-Preflight"
BASE="${STREAM_PREFLIGHT_DOWNLOADS:-https://raw.githubusercontent.com/$REPO/main/download}"
FILE="stream-preflight-linux.gz"
DEST="${HOME}/.local/bin"
BETA=0
for a in "$@"; do
  case "$a" in
    --beta) BETA=1 ;;
  esac
done

say() { printf '%s\n' "$*"; }
fail() { printf 'Stream Preflight install failed: %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Linux" ] || fail "this installer is for Linux."
case "$(uname -m)" in
  x86_64|amd64) ;;
  *) fail "only 64-bit x86 PCs are supported for now (this one is $(uname -m))." ;;
esac

if command -v curl >/dev/null 2>&1; then
  get() { curl -fsSL --retry 2 -o "$2" "$1"; }
elif command -v wget >/dev/null 2>&1; then
  get() { wget -q -O "$2" "$1"; }
else
  fail "needs curl or wget."
fi
command -v sha256sum >/dev/null 2>&1 || fail "needs sha256sum (coreutils)."
command -v gzip >/dev/null 2>&1 || fail "needs gzip."

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM
T="$(date +%s)"  # asks GitHub for a fresh copy rather than a cached one

say "Finding the newest version..."
get "$BASE/version.txt?t=$T" "$TMP/version.txt" \
  || fail "couldn't read download/version.txt from GitHub. Is the repository public and uploaded? If it was just uploaded, try again in a few minutes."

val() { sed -n "s/^[[:space:]]*$1[[:space:]]*=[[:space:]]*//p" "$TMP/version.txt" | head -n 1 | tr -d '\r' | sed 's/[[:space:]]*$//'; }

VER="$(val stable_version)"; SUM="$(val stable_sha256)"; DIR=""
if [ "$BETA" -eq 1 ] || [ -z "$VER" ]; then
  # --beta, or no stable version yet: the newest beta.
  VER="$(val beta_version)"; SUM="$(val beta_sha256)"; DIR="beta/"
fi
VER="${VER#v}"
[ -n "$VER" ] || fail "download/version.txt doesn't list a version yet."
[ "${#SUM}" -eq 64 ] || fail "download/version.txt has no checksum for v$VER."
say "Installing Stream Preflight v$VER..."

get "$BASE/$DIR$FILE?t=$T" "$TMP/$FILE" \
  || fail "couldn't download download/$DIR$FILE. If it was just uploaded, try again in a few minutes."

say "Checking the download..."
got="$(sha256sum "$TMP/$FILE" | awk '{print $1}')"
[ "$got" = "$(printf '%s' "$SUM" | tr 'A-F' 'a-f')" ] \
  || fail "the download didn't match its checksum, so nothing was installed. If a new version was just uploaded, GitHub may still be catching up: try again in a few minutes."
gzip -dc "$TMP/$FILE" > "$TMP/stream-preflight" || fail "couldn't unpack the download."
[ "$(head -c 4 "$TMP/stream-preflight" | od -An -c | tr -d ' ')" = "177ELF" ] \
  || fail "the download isn't a Linux program, so nothing was installed."

mkdir -p "$DEST"
install -m 755 "$TMP/stream-preflight" "$DEST/stream-preflight.new"
mv -f "$DEST/stream-preflight.new" "$DEST/stream-preflight"

# Menu entry and icon; also removes the old "Stream Pre-Flight Check" file.
"$DEST/stream-preflight" --install

case ":${PATH}:" in
  *":$DEST:"*) RUN="stream-preflight" ;;
  *) RUN="$DEST/stream-preflight"
     say "Note: $DEST isn't on your PATH, so type the full path or use the app menu." ;;
esac

say ""
say "Done. Open Stream Preflight from your app menu, or run: $RUN"
say "It checks for updates by itself; click Update now when it offers one."
