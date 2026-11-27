# Stream Preflight

Check your game, OBS and stream before you go live.

Stream Preflight watches a short test while you play, then tells you the one thing to change next: in your game, in OBS, or on your PC. It reads settings and measures; it never changes anything for you.

Linux (Windows builds are paused for now). Free, no account, no ads, no analytics.

## Install (Linux)

One line. It downloads the newest version from this repository, checks it against its checksum, installs it for you (no sudo) and adds it to your app menu:

```sh
curl -fsSL https://raw.githubusercontent.com/Matt-B-007/Stream-Preflight/main/install.sh | sh
```

While there are only betas, this installs the newest beta. To get the newest beta even after a stable version is out, add `-s -- --beta` after `sh`.

After that, Stream Preflight updates itself: when a new version is out, click **Update now** and it restarts on the new version. Run the line again any time to reinstall. (64-bit Linux, glibc 2.30 or newer.)

Windows builds are paused for now; the Windows code stays in the project.

## Updates and betas

The finished app lives in this repository's `download/` folder:

| File | What it is |
|---|---|
| `download/version.txt` | Which versions are there, with their SHA-256 checksums |
| `download/stream-preflight-linux.gz` | The newest stable version (once there is one) |
| `download/beta/stream-preflight-linux.gz` | The newest beta |

Stream Preflight reads `version.txt` once a day (you can turn that off in **Settings › Updates & about**). When there's a newer version, a banner offers **Update now**: it downloads the file, checks it against the checksum, swaps it in and restarts. Your settings stay. Installs and updates never depend on GitHub Releases or tags; those are only a record of each version.

- **Betas** are named after the next version: `v0.1.0-beta.4` comes before `v0.1.0`. Beta builds look for betas automatically.
- **Include beta versions** in Settings turns beta updates on or off. Any version with a dash counts as a beta.
- **Switch back to stable** installs the latest stable version, even if it's older than your beta.

## Publishing a new version (for maintainers)

1. Set `version` in `Cargo.toml` (for example `0.1.0-beta.5`) and build: `cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.31`.
2. Run `scripts/make-download.sh target/x86_64-unknown-linux-gnu/release/stream-preflight`. It puts the build in `download/` (or `download/beta/`) and updates `download/version.txt`.
3. Upload the repository to GitHub. That's it: the install line and the in-app updater pick it up within a few minutes.
4. Optional: add a tag with the same name (`v0.1.0-beta.5`), from git or with **Releases › Draft a new release › Create new tag**. The **Release** workflow checks that the tag, `Cargo.toml` and `download/version.txt` match, then publishes a GitHub Release with the notes from this changelog and the app attached. You can run the same check yourself first: `scripts/check-release.sh v0.1.0-beta.5`.

### OBS setup

In OBS: **Tools › WebSocket Server Settings › Enable WebSocket server**. Copy the password into Preflight when it asks. Preflight only reads OBS; it never starts, stops or changes anything there.

## What it does

- **Home**: a Get started card you can do in any order (nothing is locked), then one plain answer, your top fixes, last test and last stream. A bar at the top shows OBS, your game, style and platforms, each one click from changing.
- **Test**: play while OBS records (or streams and records). Preflight measures your GPU, CPU, encoder, skipped and dropped frames, and audio levels. A checklist ticks itself as OBS starts.
- **Fixes**: one change at a time, ordered by your style (Competitive, Balanced or Cinematic). Each fix says exactly where the setting is and notices when you've changed it. **Your setup** (from Home) shows OBS, game and PC settings side by side.
- **Live**: bitrate, dropped and skipped frames, each output and your recording, while you stream.
- **History**: a summary after each stream (bitrate, dropped frames, what caused them and your recording), and everything you tested and changed.
- **Games**: profiles for popular games, plus an automatic reader for Unity, Unreal, Source and Godot games from Steam, Heroic, Lutris, Bottles, Epic and Game Pass, and Minecraft from any launcher.
- **Several platforms**: Twitch, YouTube and Kick, through OBS plugins or a restream service.
- **Two PCs**: connect to OBS on your streaming PC, with capture card checks (frame rate, resolution, colour range, MJPEG, HDR, refresh rate judder). **Paired mode** links Preflight on both PCs with a 6-digit code over your home network.
- **System tray**: the icon shows OBS not connected, ready, things to fix, testing or live, with notifications when a test finishes or a stream ends. **Start at login** (off by default) and **start in the tray**. On KDE and GNOME Wayland, where a window can't hide itself, a small tray-only Preflight keeps watching OBS and opens the window when you click the icon.
- **Light while you game**: no redrawing while minimized or in the tray, and at most once a second while the game is in front. Help shows Preflight's own processor and memory use.
- **Updates**: from this repository's `download/` folder, checked against a SHA-256 checksum before installing. Turn on **Include beta versions** to get betas.

## Privacy

- Nothing is sent anywhere automatically, and there's no analytics.
- The only automatic network use: an optional daily update check (reads one small public file in this repository), and an optional upload speed test during scans. Both can be turned off in Settings.
- Never collected: stream keys, passwords, tokens, account names, emails, chat, audio, video, IP or MAC addresses, Wi-Fi names, serial numbers or product keys.
- Bug reports are saved on your PC for you to look at first. User names, home folders, Steam IDs, player names, OBS scene and source names, and addresses are removed. Reports and logs delete themselves after 14 days.
- The OBS password is encrypted with Windows' data protection on Windows, and stored in a file only you can read on Linux.
- Paired mode talks only to computers on your home network, and only after you've typed the code.

## Safe by design

- Read-only: it never edits game or OBS settings. The only files it deletes are its own test recordings (when you choose to) and, when you uninstall, its own files.
- Anti-cheat friendly: no game memory access, no injection, no hooks.
- Never controls OBS.

## Building from source

Needs Rust 1.85 or newer.

```sh
cargo build --release
cargo test
```

Linux release builds use `cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.31` for compatibility with older distributions.

## Uninstall

**Settings › Advanced › Remove Preflight from this PC** shows exactly what will be deleted, then removes Preflight's own settings, history, logs and reports, its menu entry, and (if you tick it) the program file. On Windows you can also uninstall from **Settings › Apps**. OBS and your games are never touched.
