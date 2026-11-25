# Changelog

## v0.1.0-beta.3

- **First-time welcome** in three steps: what Preflight does, connect OBS, and how you stream. Empty states and one-time hints so nothing looks broken before the first test.
- **In-flight**: a page just for when you're live. It opens by itself when you start streaming (never over your game): live time, one verdict, bitrate, dropped and skipped frames, recording, a 10-minute bitrate graph and plain notes when something goes wrong.
- **Sidebar**: Pre-flight, In-flight and Post-flight. It folds to icons with names on hover and badges (Ctrl+B), and folds by itself on narrow windows.
- **Simpler screens**: one message per screen, one verdict at the top, the test choice folded to one line, and the fix shown as an instruction first with quiet choices underneath and an "All fixes" list.
- **Settings in five tabs**: General, OBS & PCs, Tray & notifications, Privacy & data, Updates & about.
- **Betas**: any version with a dash counts as a beta; beta builds look for betas; "Switch back to stable".
- **Install**: the one-line command finds the newest release, including betas when there's no stable release yet.
- **GitHub**: checks that build and test every push, and a release workflow that builds and publishes the Linux download from a tag.
- Linux only for now.

## v0.1.1

- **One-line install**: `install.sh` for Linux and `install.ps1` for Windows download the newest release, check it against SHA256SUMS.txt, install it for you and add the menu or Start menu entry.
- **Update now**: when a new version is out, a banner on the main screen updates and restarts Preflight in one click. A notification appears if Preflight is in the tray.
- `--install` and `--version` options for scripts.

## v0.1.0 (first release)

The first public version, after 13 test builds.

- **New name**: Stream Preflight (was "Stream Pre-Flight Check"). Settings, history and the saved OBS password move over automatically.
- **New look**: a live readings strip, score dials, a glowing "Do this next" card, and one bright button in the Next bar.
- **Sidebar**: Pre-flight opens into your game, its style, and the steps Test, Fix, Your setup and Ready to go live. Post-flight holds Last stream and History.
- **Test step**: one checklist that ticks itself as OBS starts streaming and recording.
- **Two PCs**: connect to OBS on another PC, capture card and NDI checks, and fixes labelled "On your gaming PC" or "On your streaming PC".
- **Paired mode**: Preflight on both PCs, linked with a 6-digit code, home network only.
- **System tray** with status colours and notifications; "Is this a real stream?" when the Twitch test flag may still be on.
- **Install, update and uninstall**: per-user install with a menu entry (and Windows Settings › Apps entry), updates from GitHub Releases checked against SHA256SUMS, optional beta versions.
- **Bug reports** include post-flight numbers, test history, one setup for all games, what was on screen, the tray and two-PC details, with personal details removed.
