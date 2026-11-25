#!/bin/sh
# Stream Preflight installer for Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/Matt-B-007/Stream-Preflight/main/scripts/install.sh | sh
#
# Finds the newest release (the newest stable one, or the newest beta while
# there's no stable release yet), downloads it, checks it against the
# release's SHA256SUMS.txt, puts it in ~/.local/bin and adds it to your app
# menu. No sudo. Add "-s -- --beta" after "sh" to get the newest beta even
# when there's a stable release. After that, Stream Preflight updates itself.
set -eu

REPO="Matt-B-007/Stream-Preflight"
API="${STREAM_PREFLIGHT_API:-https://api.github.com/repos/$REPO/releases}"
DL="${STREAM_PREFLIGHT_DL:-https://github.com/$REPO/releases/download}"
FILE="stream-preflight-linux-x86_64"
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
  fetch() { curl -fsSL --retry 2 -H "Accept: application/vnd.github+json" "$1"; }
  get() { curl -fsSL --retry 2 -o "$2" "$1"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -q -O - --header="Accept: application/vnd.github+json" "$1"; }
  get() { wget -q -O "$2" "$1"; }
else
  fail "needs curl or wget."
fi
command -v sha256sum >/dev/null 2>&1 || fail "needs sha256sum (coreutils)."

first_tag() { sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1; }

say "Finding the newest release..."
TAG=""
if [ "$BETA" -eq 0 ]; then
  TAG="$(fetch "$API/latest" 2>/dev/null | first_tag || true)"
fi
if [ -z "$TAG" ]; then
  # No stable release yet (or --beta): the newest release of any kind.
  TAG="$(fetch "$API?per_page=20" | tr ',' '\n' | first_tag || true)"
fi
[ -n "$TAG" ] || fail "couldn't find a release. Is the repository public, and is a release published?"
say "Installing Stream Preflight $TAG..."

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM

get "$DL/$TAG/$FILE" "$TMP/$FILE" || fail "couldn't download $FILE from release $TAG."
get "$DL/$TAG/SHA256SUMS.txt" "$TMP/SHA256SUMS.txt" || fail "couldn't download SHA256SUMS.txt from release $TAG."

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
