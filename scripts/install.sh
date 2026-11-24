#!/bin/sh
# Stream Preflight installer for Linux.
#   curl -fsSL https://github.com/Matt-B-007/Stream-Preflight/releases/latest/download/install.sh | sh
# Downloads the newest release, checks it against SHA256SUMS.txt, puts it in
# ~/.local/bin and adds it to your app menu. No sudo. Run it again any time to
# reinstall; after that, Stream Preflight updates itself from inside the app.
set -eu

REPO="Matt-B-007/Stream-Preflight"
BASE="${STREAM_PREFLIGHT_BASE:-https://github.com/$REPO/releases/latest/download}"
FILE="stream-preflight-linux-x86_64"
DEST="${HOME}/.local/bin"

say() { printf '%s\n' "$*"; }
fail() { printf 'Stream Preflight install failed: %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Linux" ] || fail "this installer is for Linux. On Windows, use the PowerShell command in the README."
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

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM

say "Downloading Stream Preflight..."
get "$BASE/$FILE" "$TMP/$FILE" || fail "couldn't download $FILE."
get "$BASE/SHA256SUMS.txt" "$TMP/SHA256SUMS.txt" || fail "couldn't download SHA256SUMS.txt."

say "Checking the download..."
want="$(grep " \*\{0,1\}$FILE\$" "$TMP/SHA256SUMS.txt" | awk '{print $1}')"
[ -n "$want" ] || fail "SHA256SUMS.txt doesn't list $FILE."
got="$(sha256sum "$TMP/$FILE" | awk '{print $1}')"
[ "$want" = "$got" ] || fail "the download didn't match its checksum. Nothing was installed."

mkdir -p "$DEST"
install -m 755 "$TMP/$FILE" "$DEST/stream-preflight.new"
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
