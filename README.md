# Stream Preflight

Check your game, OBS and stream before you go live.

Stream Preflight watches a short test while you play, then tells you the one thing to change next: in your game, in OBS, or on your PC. It reads settings and measures; it never changes anything for you.

Windows and Linux. Free, no account, no ads, no analytics.

## Download

Get the latest version from the [Releases page](https://github.com/Matt-B-007/Stream-Preflight/releases).

| System | File |
|---|---|
| Windows 10/11 (64-bit) | `stream-preflight-windows-x86_64.exe` |
| Linux (64-bit, glibc 2.31 or newer) | `stream-preflight-linux-x86_64` |

`SHA256SUMS.txt` lists the checksum of each file.

### Install on Windows

1. Download `stream-preflight-windows-x86_64.exe` and run it.
2. Windows SmartScreen may warn you because the file isn't signed yet: click **More info › Run anyway**.
3. In Preflight, open **Settings › Advanced › Install** and click **Install for this user**. It copies itself to your user folder, adds a Start menu shortcut, and appears in **Settings › Apps** so you can uninstall it there. No admin rights needed.

### Install on Linux

```sh
mkdir -p ~/.local/bin
mv ~/Downloads/stream-preflight-linux-x86_64 ~/.local/bin/stream-preflight
chmod +x ~/.local/bin/stream-preflight
~/.local/bin/stream-preflight
```

Or run the downloaded file once and use **Settings › Advanced › Install**, which does the same and adds Preflight to your app menu.

### OBS setup

In OBS: **Tools › WebSocket Server Settings › Enable WebSocket server**. Copy the password into Preflight when it asks. Preflight only reads OBS; it never starts, stops or changes anything there.

## What it does

- **Pre-flight**: a guided check before you stream.
  - **Test**: play while OBS records (or streams and records). Preflight measures your GPU, CPU, encoder, skipped and dropped frames, and audio levels. A checklist ticks itself as OBS starts.
  - **Fix**: one change at a time, ordered by your style (Competitive, Balanced or Cinematic). Each fix says exactly where the setting is and notices when you've changed it.
  - **Your setup**: OBS, game and PC settings side by side, video encoders in use, and one OBS setup that suits all your tested games.
  - **Ready to go live**: mic, game audio, capture, webcam and the Twitch test flag, checked before you start.
- **Post-flight**: a summary after each stream (bitrate, dropped frames and what caused them), and a history.
- **Games**: profiles for popular games, plus an automatic reader for Unity, Unreal, Source and Godot games from Steam, Heroic, Lutris, Bottles, Epic and Game Pass, and Minecraft from any launcher.
- **Several platforms**: Twitch, YouTube and Kick, through OBS plugins or a restream service.
- **Two PCs**: connect to OBS on your streaming PC, with capture card checks (frame rate, resolution, colour range, MJPEG, HDR, refresh rate judder). **Paired mode** links Preflight on both PCs with a 6-digit code over your home network.
- **System tray**: the icon shows idle, watching, testing or live, with notifications when a test finishes or a stream ends.
- **Updates**: from this repository's releases, checked against `SHA256SUMS.txt` before installing. Turn on **Include beta versions** to get pre-releases.

## Privacy

- Nothing is sent anywhere automatically, and there's no analytics.
- The only automatic network use: an optional daily update check (reads this repository's public release list), and an optional upload speed test during scans. Both can be turned off in Settings.
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
