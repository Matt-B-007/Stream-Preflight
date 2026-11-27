# Contributing to Stream Preflight

Thanks for helping. Bug reports, game profiles and fixes are all welcome.

## Ground rules

Every change has to keep these:

- **Read-only**: never edit game or OBS settings. The only files Preflight deletes are its own test recordings and its own files.
- **Never controls OBS**, and never does anything that could trip anti-cheat (no game memory access, injection or hooks).
- **Informs, doesn't decide**: it explains a change and where to make it; the streamer makes it.
- **Privacy**: never collect stream keys, tokens, passwords, account names, emails, chat, audio or video, IP or MAC addresses, Wi-Fi names, serial numbers or product keys. Never read OBS's `service.json` or plugin settings files. Nothing is sent anywhere automatically.

## Making a change

1. Install the tools (on Arch or CachyOS): `sudo pacman -S --needed rustup cargo-deny shellcheck python-yaml`, then `rustup toolchain install` in the project folder.
2. Make the change. Keep a pure tidy-up (moving or renaming code) separate from a change in behaviour, so either is easy to review and to undo.
3. Run `sh scripts/check.sh`. It runs exactly what CI runs; if it passes, CI passes. Exit code 0 = passed, 1 = a check failed, 3 = only tools are missing.
4. Open a pull request and fill in the template.

## Style

- Rust: `cargo fmt`, and clippy with no warnings.
- Shell scripts: POSIX `sh` unless there's a reason for bash. They must pass `sh -n` (or `bash -n`) and shellcheck at its default severity. Don't silence a finding without a comment explaining why. Use `if`/`then` rather than `a && b || c`.
- New source files start with `// SPDX-License-Identifier: GPL-3.0-or-later` (or `#` in scripts, after the first line).
- Words in the app: short, plain and specific. Say what to do and where the setting is.

## Licence

By contributing, you agree that your contribution is licensed under GPL-3.0-or-later, like the rest of the project. New dependencies must use a licence listed in `deny.toml`.
