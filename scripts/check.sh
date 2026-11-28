#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Every check CI runs, in the same order with the same flags. CI runs only
# this script for checks, and StowLock runs it before pushing, so a pass here
# is a pass there.
#
#   sh scripts/check.sh
#
# Exit codes: 0 = all checks passed, 1 = a check failed, 3 = only tools are
# missing (nothing was checked; the list says how to install them).
set -eu

cd "$(dirname "$0")/.." || exit 1

# The version CI pins. A different local version still runs, with a note.
SHELLCHECK_CI_VERSION=0.11.0
DENY_CI_VERSION=0.22.2

# Colour only on a terminal, and only when NO_COLOR isn't set.
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    bold="$(printf '\033[1m')"
    red="$(printf '\033[31m')"
    green="$(printf '\033[32m')"
    reset="$(printf '\033[0m')"
else
    bold=""
    red=""
    green=""
    reset=""
fi

step() {
    printf '\n%s==> %s%s\n' "$bold" "$1" "$reset"
}

# ---- 1. Every tool first -----------------------------------------------------
missing=""
pacman_pkgs=""
need() {
    # need <what> <arch package> <other way to get it>
    missing="$missing\n  - $1\n      Other systems: $3"
    case " $pacman_pkgs " in
        *" $2 "*) ;;
        *) pacman_pkgs="$pacman_pkgs $2" ;;
    esac
}

if ! command -v cargo >/dev/null 2>&1; then
    need "Rust (cargo), via rustup" "rustup" "https://rustup.rs, then run 'rustup toolchain install' in this folder"
else
    # rust-toolchain.toml pins the version; rustup installs it with rustfmt and clippy.
    if ! cargo fmt --version >/dev/null 2>&1; then
        need "rustfmt for the pinned Rust (rust-toolchain.toml)" "rustup" "run 'rustup toolchain install' in this folder"
    fi
    if ! cargo clippy --version >/dev/null 2>&1; then
        need "clippy for the pinned Rust (rust-toolchain.toml)" "rustup" "run 'rustup toolchain install' in this folder"
    fi
fi
if ! command -v cargo-deny >/dev/null 2>&1; then
    need "cargo-deny (checks dependency licences; CI uses $DENY_CI_VERSION)" "cargo-deny" "cargo install cargo-deny --locked --version $DENY_CI_VERSION"
fi
if ! command -v shellcheck >/dev/null 2>&1; then
    need "shellcheck (CI uses $SHELLCHECK_CI_VERSION)" "shellcheck" "pip install shellcheck-py==$SHELLCHECK_CI_VERSION.1"
fi
if ! command -v python3 >/dev/null 2>&1; then
    need "Python 3 (checks the workflow files)" "python" "your system's python3 package"
elif ! python3 -c 'import yaml' >/dev/null 2>&1; then
    need "PyYAML for Python 3 (checks the workflow files)" "python-yaml" "pip install pyyaml"
fi
for f in install.sh uninstall.sh get.sh scripts/*.sh tools/*; do
    [ -f "$f" ] || continue
    if head -n 1 "$f" | grep -q bash && ! command -v bash >/dev/null 2>&1; then
        need "bash (some scripts use it)" "bash" "your system's bash package"
        break
    fi
done

if [ -n "$missing" ]; then
    printf '%sSome tools are missing, so nothing was checked.%s\n' "$red" "$reset" >&2
    printf '\nOn Arch or CachyOS, install them with:\n  sudo pacman -S --needed%s\n' "$pacman_pkgs" >&2
    if case " $pacman_pkgs " in *" rustup "*) true ;; *) false ;; esac then
        printf "  then, in this folder: rustup toolchain install\n" >&2
    fi
    printf '\nMissing:%b\n' "$missing" >&2
    exit 3
fi

# ---- 2. The checks -----------------------------------------------------------
failed=""
fail() {
    printf '%sFAILED: %s%s\n' "$red" "$1" "$reset" >&2
    failed="$failed\n  - $1"
}

step "Check formatting (cargo fmt --all -- --check)"
if ! cargo fmt --all -- --check; then
    fail "formatting: run 'cargo fmt' and check again"
fi

step "Lint (cargo clippy --all-targets --locked -- -D warnings)"
if ! cargo clippy --all-targets --locked -- -D warnings; then
    fail "clippy found problems"
fi

step "Build (cargo build --locked)"
if ! cargo build --locked; then
    fail "build"
fi

step "Tests (cargo test --locked)"
if ! cargo test --locked; then
    fail "tests"
fi

step "Program self-test (target/debug/stream-preflight --self-test --quick)"
if [ ! -x target/debug/stream-preflight ]; then
    fail "self-test: the program wasn't built"
elif ! target/debug/stream-preflight --self-test --quick; then
    fail "self-test"
fi

step "Dependency licences (cargo deny check licenses)"
if ! cargo deny check licenses; then
    fail "a dependency's licence isn't on the allow-list in deny.toml"
fi

have="$(shellcheck --version | sed -n 's/^version: //p')"
step "Check shell scripts (sh -n or bash -n, then shellcheck $have)"
if [ "$have" != "$SHELLCHECK_CI_VERSION" ]; then
    printf '  Note: CI uses shellcheck %s; this is %s, so findings may differ slightly.\n' "$SHELLCHECK_CI_VERSION" "$have"
fi
for f in install.sh uninstall.sh get.sh scripts/*.sh tools/*; do
    [ -f "$f" ] || continue
    # Shell scripts only: a .sh name or a shell on the first line. A Python
    # tool in tools/ is not a shell script, so it's listed and left out.
    first="$(head -n 1 "$f")"
    case "$f:$first" in
        *:*bash*) shell="bash" ;;
        *.sh:* | *:'#!/bin/sh'* | *:'#!/usr/bin/env sh'*) shell="sh" ;;
        *)
            printf '  %s (not a shell script)\n' "$f"
            continue
            ;;
    esac
    printf '  %s (%s)\n' "$f" "$shell"
    if ! "$shell" -n "$f"; then
        fail "$f: $shell -n found a syntax error"
    elif ! shellcheck -s "$shell" "$f"; then
        fail "$f: shellcheck findings"
    fi
done

step "Check GitHub files (YAML parses)"
for f in .github/workflows/*.yml .github/ISSUE_TEMPLATE/*.yml; do
    [ -f "$f" ] || continue
    printf '  %s\n' "$f"
    if ! python3 -c 'import sys, yaml; yaml.safe_load(open(sys.argv[1]))' "$f"; then
        fail "$f doesn't parse"
    fi
done

if [ -n "$failed" ]; then
    printf '\n%sChecks failed:%b%s\n' "$red" "$failed" "$reset" >&2
    exit 1
fi
printf '\n%sAll checks passed.%s\n' "$green" "$reset"
