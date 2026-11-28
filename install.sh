#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Stream Preflight installer for Linux. One line:
#
#   curl -fsSL https://raw.githubusercontent.com/Matt-B-007/Stream-Preflight/main/install.sh | sh
#
# Options (after "sh -s --"):
#   --beta           newest beta, even when there's a stable version
#   --version X      a specific version, e.g. --version 0.1.0-beta.8
#   --no-launch      don't start Stream Preflight afterwards
#   --from-source    build from source with cargo instead of downloading the program
#   --uninstall      remove Stream Preflight (add --delete-settings to remove settings and logs too)
#
# Downloads from GitHub Releases, checks the program against SHA256SUMS, and
# checks SHA256SUMS's signature when minisign is installed. Installs to
# ~/.local/bin and adds an app menu entry. No sudo.
set -eu

REPO="Matt-B-007/Stream-Preflight"
API="${STREAM_PREFLIGHT_API:-https://api.github.com/repos/$REPO/releases?per_page=30}"
LEGACY="${STREAM_PREFLIGHT_DOWNLOADS:-https://raw.githubusercontent.com/$REPO/main/download}"
ASSET="stream-preflight-linux.gz"
DEST="${HOME}/.local/bin"
# The maintainer's public keys (the same two the app trusts).
KEY_PRIMARY="RWQcs3jtP2jTiAMafwQX8UwciSjOH4o9tQcCVvYl2NkdnOeVupKPlhvC"
KEY_BACKUP="RWRObC9qlP598z1kmTFw7W+TKYKk3w2/63qWyRWkvRSkqGwGEgjmOlGe"

BETA=0
WANT=""
LAUNCH=1
SOURCE=0
UNINSTALL=0
PURGE=0
while [ $# -gt 0 ]; do
    case "$1" in
        --beta) BETA=1 ;;
        --version)
            shift
            WANT="${1:-}"
            ;;
        --version=*) WANT="${1#--version=}" ;;
        --no-launch) LAUNCH=0 ;;
        --from-source) SOURCE=1 ;;
        --uninstall) UNINSTALL=1 ;;
        --delete-settings) PURGE=1 ;;
        *) echo "Unknown option: $1 (see the top of install.sh)" >&2; exit 2 ;;
    esac
    shift
done
WANT="${WANT#v}"

say() { printf '%s\n' "$*"; }
fail() { printf 'Stream Preflight install failed: %s\n' "$*" >&2; exit 1; }

if [ "$UNINSTALL" -eq 1 ]; then
    if [ -x "$DEST/stream-preflight" ]; then
        if [ "$PURGE" -eq 1 ]; then
            "$DEST/stream-preflight" --uninstall --delete-settings --yes
        else
            "$DEST/stream-preflight" --uninstall --yes
        fi
        exit $?
    fi
    fail "Stream Preflight isn't installed in $DEST."
fi

[ "$(uname -s)" = "Linux" ] || fail "this installer is for Linux."
case "$(uname -m)" in
    x86_64 | amd64) ;;
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

# ---- Pick a release ------------------------------------------------------------
# A release counts only with all three files: the program, SHA256SUMS and its signature.
say "Finding the newest version..."
TAG=""
if get "$API" "$TMP/releases.json"; then
    # Every download link is .../releases/download/<tag>/<file>.
    # One JSON field per line first: the API may send everything on one line.
    tr ',{}[]' '[\n*5]' < "$TMP/releases.json" |
        sed -n 's|^[[:space:]]*"browser_download_url": *"\([^"]*\)".*|\1|p' > "$TMP/urls"
    sed -n 's|.*/releases/download/\([^/]*\)/.*|\1|p' "$TMP/urls" | sort -u > "$TMP/tags"
    while IFS= read -r t; do
        n=0
        for f in "$ASSET" SHA256SUMS SHA256SUMS.minisig; do
            if grep -q "/releases/download/$t/$f\$" "$TMP/urls"; then
                n=$((n + 1))
            fi
        done
        if [ "$n" -eq 3 ]; then
            printf '%s\n' "$t" >> "$TMP/complete"
        fi
    done < "$TMP/tags"
    if [ -s "$TMP/complete" ]; then
        if [ -n "$WANT" ]; then
            if grep -qx "v$WANT" "$TMP/complete"; then
                TAG="v$WANT"
            else
                fail "version $WANT isn't on GitHub Releases (or isn't signed). Versions there: $(tr '\n' ' ' < "$TMP/complete")"
            fi
        else
            # Betas have a dash. With '~' in place of '-', sort -V puts 0.1.0~beta.8 before 0.1.0.
            if [ "$BETA" -eq 1 ]; then
                pick="$(cat "$TMP/complete")"
            else
                pick="$(grep -v -- '-' "$TMP/complete" || true)"
                # Only betas so far: use the newest beta.
                [ -n "$pick" ] || pick="$(cat "$TMP/complete")"
            fi
            TAG="$(printf '%s\n' "$pick" | tr '-' '~' | sort -V | tail -n 1 | tr '~' '-')"
        fi
    fi
fi

if [ -z "$TAG" ]; then
    [ -z "$WANT" ] || fail "couldn't read GitHub Releases to find version $WANT."
    # During the move to GitHub Releases, older versions are still in the repository.
    say "No signed release found on GitHub Releases yet; using the repository's download folder."
    T="$(date +%s)"
    get "$LEGACY/version.txt?t=$T" "$TMP/version.txt" || fail "couldn't read GitHub. Check your internet connection and try again."
    val() { sed -n "s/^[[:space:]]*$1[[:space:]]*=[[:space:]]*//p" "$TMP/version.txt" | head -n 1 | tr -d '\r' | sed 's/[[:space:]]*$//'; }
    VER="$(val stable_version)"
    SUM="$(val stable_sha256)"
    DIR=""
    if [ "$BETA" -eq 1 ] || [ -z "$VER" ]; then
        VER="$(val beta_version)"
        SUM="$(val beta_sha256)"
        DIR="beta/"
    fi
    VER="${VER#v}"
    [ -n "$VER" ] || fail "no version is available yet."
    [ "$SOURCE" -eq 0 ] || fail "--from-source needs a release on GitHub Releases."
    say "Installing Stream Preflight v$VER..."
    get "$LEGACY/$DIR$ASSET?t=$T" "$TMP/$ASSET" || fail "couldn't download the program. Try again in a few minutes."
    got="$(sha256sum "$TMP/$ASSET" | awk '{print $1}')"
    [ "$got" = "$SUM" ] || fail "the download didn't match its checksum, so nothing was installed."
else
    VER="${TAG#v}"
    # The folder GitHub lists for this release's files.
    BASE="$(grep "/releases/download/$TAG/SHA256SUMS\$" "$TMP/urls" | head -n 1)"
    BASE="${BASE%/SHA256SUMS}"
    if [ "$SOURCE" -eq 1 ]; then
        command -v cargo >/dev/null 2>&1 || fail "--from-source needs Rust (on Arch: sudo pacman -S --needed rustup, then rustup default stable)."
        command -v tar >/dev/null 2>&1 || fail "--from-source needs tar."
        say "Building Stream Preflight v$VER from source (this takes a few minutes)..."
        get "https://github.com/$REPO/archive/refs/tags/$TAG.tar.gz" "$TMP/src.tar.gz" || fail "couldn't download the source."
        mkdir -p "$TMP/src"
        tar -xzf "$TMP/src.tar.gz" -C "$TMP/src" --strip-components=1
        (cd "$TMP/src" && cargo build --release --locked) || fail "the build failed."
        cp "$TMP/src/target/release/stream-preflight" "$TMP/stream-preflight"
    else
        say "Installing Stream Preflight v$VER..."
        get "$BASE/SHA256SUMS" "$TMP/SHA256SUMS" || fail "couldn't download SHA256SUMS."
        get "$BASE/SHA256SUMS.minisig" "$TMP/SHA256SUMS.minisig" || fail "couldn't download the signature."
        if command -v minisign >/dev/null 2>&1; then
            if minisign -V -q -P "$KEY_PRIMARY" -m "$TMP/SHA256SUMS" >/dev/null 2>&1 || minisign -V -q -P "$KEY_BACKUP" -m "$TMP/SHA256SUMS" >/dev/null 2>&1; then
                say "Signature checked."
            else
                fail "the release isn't signed by Stream Preflight's maintainer, so nothing was installed."
            fi
        else
            say "Note: minisign isn't installed, so only the checksum is checked (on Arch: sudo pacman -S minisign)."
        fi
        get "$BASE/$ASSET" "$TMP/$ASSET" || fail "couldn't download the program. Try again in a few minutes."
        SUM="$(awk -v f="$ASSET" '$2 == f || $2 == "*" f { print $1 }' "$TMP/SHA256SUMS")"
        [ "${#SUM}" -eq 64 ] || fail "SHA256SUMS doesn't list the program."
        got="$(sha256sum "$TMP/$ASSET" | awk '{print $1}')"
        [ "$got" = "$SUM" ] || fail "the download didn't match its checksum, so nothing was installed."
    fi
fi

if [ ! -f "$TMP/stream-preflight" ]; then
    gzip -dc "$TMP/$ASSET" > "$TMP/stream-preflight" || fail "couldn't unpack the download."
fi
[ "$(head -c 4 "$TMP/stream-preflight" | od -An -c | tr -d ' ')" = "177ELF" ] || fail "the download isn't a Linux program, so nothing was installed."
chmod 755 "$TMP/stream-preflight"
# Newer versions can check themselves before going in place.
if "$TMP/stream-preflight" --version >/dev/null 2>&1; then
    case "$("$TMP/stream-preflight" --version)" in
        0.1.0-beta.[1-7]) ;;
        *)
            "$TMP/stream-preflight" --self-test --quick >/dev/null 2>&1 || fail "the program failed its self-test, so nothing was installed."
            ;;
    esac
fi

mkdir -p "$DEST"
install -m 755 "$TMP/stream-preflight" "$DEST/stream-preflight.new"
mv -f "$DEST/stream-preflight.new" "$DEST/stream-preflight"

# Menu entry and icon; also removes the old "Stream Pre-Flight Check" file.
"$DEST/stream-preflight" --install

case ":${PATH}:" in
    *":$DEST:"*) RUN="stream-preflight" ;;
    *)
        RUN="$DEST/stream-preflight"
        say "Note: $DEST isn't on your PATH, so type the full path or use the app menu."
        ;;
esac

say ""
say "Done. Stream Preflight v$VER is installed."
if [ "$LAUNCH" -eq 1 ] && { [ -n "${WAYLAND_DISPLAY:-}" ] || [ -n "${DISPLAY:-}" ]; }; then
    say "Starting it now. Next time, open it from your app menu or run: $RUN"
    nohup "$DEST/stream-preflight" >/dev/null 2>&1 &
else
    say "Open it from your app menu, or run: $RUN"
fi
say "It checks for updates by itself; click Update now when it offers one."
