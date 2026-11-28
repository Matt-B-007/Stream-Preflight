# Security

## Reporting a problem

Please report security problems privately, using GitHub's **Report a vulnerability** button under this repository's **Security** tab. Don't open a public issue. You'll get a reply within a week.

Useful to include: the version (`stream-preflight --version`), what you found, and how to reproduce it.

## Supported versions

Only the newest release (stable or beta) gets fixes. Update with **Settings › Updates › Check now**.

## How updates are protected

- Releases are built by GitHub Actions from a tagged commit, never uploaded by hand.
- Each release has a `SHA256SUMS` file signed with minisign. The app has two public keys built in (a main key and an offline backup key) and accepts a signature from either:
  - `RWQcs3jtP2jTiAMafwQX8UwciSjOH4o9tQcCVvYl2NkdnOeVupKPlhvC` (main)
  - `RWRObC9qlP598z1kmTFw7W+TKYKk3w2/63qWyRWkvRSkqGwGEgjmOlGe` (backup)
- The app installs an update only when the download matches `SHA256SUMS`, the signature is valid and the new version passes its self-test. The previous version is kept so you can go back.

To check a release yourself:

```sh
minisign -V -P RWQcs3jtP2jTiAMafwQX8UwciSjOH4o9tQcCVvYl2NkdnOeVupKPlhvC -m SHA256SUMS
sha256sum -c SHA256SUMS --ignore-missing
```

## What Preflight never does

It never collects stream keys, tokens, passwords or account details, never reads OBS's `service.json`, never changes game or OBS settings, and sends nothing anywhere automatically.
