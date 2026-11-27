#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Checks FILE.minisig against the public keys embedded in the app
# (src/update.rs). Passes if either key matches. Used by the Release
# workflow before anything is published.
#
#   sh scripts/verify-signature.sh dist/SHA256SUMS
set -eu

file="${1:?usage: scripts/verify-signature.sh <file> (its .minisig must sit next to it)}"
cd "$(dirname "$0")/.." || exit 1
[ -f "$file" ] || { echo "Missing: $file" >&2; exit 1; }
[ -f "$file.minisig" ] || { echo "Missing: $file.minisig" >&2; exit 1; }

keys="$(sed -n 's/.*"\(RW[A-Za-z0-9+/]\{54\}\)".*/\1/p' src/update.rs)"
count="$(printf '%s\n' "$keys" | grep -c . || true)"
if [ "$count" -lt 2 ]; then
    echo "Expected two public keys in src/update.rs, found $count." >&2
    exit 1
fi

n=0
for k in $keys; do
    n=$((n + 1))
    if minisign -V -q -P "$k" -m "$file" >/dev/null 2>&1; then
        echo "Signature OK: matches embedded key $n of $count."
        exit 0
    fi
done
echo "::error::The signature doesn't match either embedded public key. Check that the MINISIGN_SECRET_KEY secret is the primary key." >&2
exit 1
