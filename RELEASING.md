# Releasing Stream Preflight

A release is a signed GitHub Release made by the **Release** workflow when a version tag is pushed. Nobody uploads the program by hand.

## Steps

1. **Version**: set `version` in `Cargo.toml` (for example `0.1.0-beta.9`), then update `Cargo.lock` with `cargo update -p stream-preflight --offline`.
2. **Changelog**: add `## v0.1.0-beta.9` at the top of `CHANGELOG.md`: one line saying what the release is about, then bullets with bold leads. The release notes are taken from this section.
3. **Check**: `sh scripts/check.sh` must pass (exit code 0), then `sh scripts/check-release.sh v0.1.0-beta.9`.
4. **Upload** the repository (StowLock runs `scripts/check.sh` first).
5. **Tag**: push the tag `v0.1.0-beta.9` (StowLock can push it, or use **Releases › Draft a new release › Create new tag**, then close the page without publishing). A tag with a dash is published as a pre-release.

## What the Release workflow does

1. Stops at once if the `MINISIGN_SECRET_KEY` secret is missing.
2. Runs `scripts/check.sh` and `scripts/check-release.sh` (the tag must match `Cargo.toml` and the top of the changelog).
3. Builds with `cargo zigbuild --release --locked --target x86_64-unknown-linux-gnu.2.31` and runs the build's `--self-test --quick`.
4. Packs `stream-preflight-linux.gz`, `install.sh` and `SHA256SUMS`, signs `SHA256SUMS` with minisign (the key is written to a temporary file, used, and deleted; it's never printed), then checks the signature against both public keys in `src/update.rs`.
5. Adds build provenance (public repositories only).
6. Creates a **draft** release, uploads the files, then publishes it. Until it's published, the app and the installer can't see it.

If a step fails, the run's summary says whether a draft was left behind. Delete it under **Releases** (or push the tag again after fixing the problem: an unfinished draft with the same tag is replaced automatically).

**"Error while loading the secret key file"** means the secret's contents are wrong (for example a missing line, or a password-protected key). Fix the secret under **Settings › Secrets and variables › Actions**; the workflow itself is fine.

## Keys

- The app and the installer trust two public keys: the main key (its secret is the `MINISIGN_SECRET_KEY` repository secret) and a backup key whose secret is kept offline and never in GitHub.
- If the main key is ever lost or exposed, sign with the backup key, then ship a version with a new main key.

## The `download/` folder (transition)

Versions up to beta.7 update from `download/version.txt` in this repository. beta.8 is the last version placed there (with `scripts/make-download.sh`), so they can update to a version that knows about signed releases. `download/` stays frozen at beta.8 for at least two releases, and is then deleted; the changelog records when.
