# Changelog

## v0.1.0-beta.7

The first clippy cleanup and a single check script, on their own so anything they break is easy to trace. Nothing looks or works differently.

- **Clippy now passes with warnings as errors**: 157 warnings fixed, mostly nested `if`s joined into one, plus type names for long tuple types and a few small tidy-ups. Each fix keeps the same behaviour; every screen was compared pixel by pixel against beta.6.
- **One check script, `scripts/check.sh`**: formatting, clippy, build, tests, shell scripts (`sh -n`/`bash -n` and shellcheck at default severity) and workflow files, in that order. CI runs only this script, so a local pass means a CI pass. It checks for every tool first and lists any missing ones with the Arch command (`sudo pacman -S --needed …`); exit code 0 = passed, 1 = a check failed, 3 = only tools missing. A shellcheck version other than CI's still runs, with a note.
- **Pinned tools**: Rust 1.97.0 in `rust-toolchain.toml`, shellcheck 0.11.0 and PyYAML 6.0.3 in CI, so a new tool release can't break CI overnight.
- **CI steps say what they run**, and use the current major versions of GitHub's actions.

## v0.1.0-beta.6

A behind-the-scenes release: the code is reorganised so the next updates are safer to make. Nothing looks or works differently.

- **The window code is split by screen**: the 8,200-line `app.rs` is now `src/app/` with one file per screen (Home, Test, Fixes, Your setup, Live, History, Settings, Help) plus shared state, the frame around every screen, the tray, two-PC support and small drawing helpers. This makes each screen easier to change without touching the others.
- **The code is formatted with `cargo fmt`**, ready for the formatting check that CI gains in the next release.
- **Checked unchanged**: every screen was compared pixel by pixel against beta.5 (only the clock differed), and all 75 tests pass.

## v0.1.0-beta.5

**Easier to find your way**
- Opens on **Home**. No forced welcome: a **Get started** card (how you stream, connect OBS, start your game, first test) you can do in any order, tick off by itself, or hide.
- Home leads with one plain answer, then your top fixes, last test and last stream.
- A simpler sidebar: **Home, Test, Fixes, Live, History**, plus Settings and Help. Alt+1 to Alt+5 jump between them; Ctrl+B folds it.
- A **top bar** for OBS, your game, style and platforms, each one click from changing, and whether you're live.
- Nothing is locked: pages that need OBS show a note with a Connect OBS button.

**Lighter while you game (same look)**
- No redrawing while minimized or in the tray; at most once a second while another window is in front. Animations step once a second.
- The background is built once and reused. Game settings files are only re-read when something changed. Game profiles are loaded once.
- Help shows Preflight's own processor and memory use; stream summaries record memory at the start and end.

**Recording while streaming**
- The Live page asks OBS every 5 seconds whether it's recording (this fixes Hybrid MOV and MP4 recordings showing as "Not recording"), with the recording's timer and rate.
- The Live page's encoder and "Each output" numbers now come live from OBS, not from before the stream.
- The stream summary says whether you recorded, for how long and at what rate; Recording space uses that real rate.
- Your setup shows OBS's "Automatically record when streaming" setting.

**Tray and startup**
- Start at login (off by default) and start in the tray. When you go live: stay in the tray, or open the Live page.
- The tray icon shows OBS not connected, ready, things to fix, testing or live.
- On KDE/GNOME Wayland, going to the tray closes the window and a small tray-only Preflight keeps watching OBS; clicking the icon opens the window again.

**Fixes from bug reports**
- Web views, browsers, launchers and desktop programs are no longer taken for games; old entries like "webkitwebprocess" are removed. A test with no game says so instead of scoring the game.
- Linux capture cards show the right resolution (OBS packs it in 32-bit halves).
- Two Desktop Audio sources on the same device are flagged (viewers may hear the game twice).
- Mic vs game says "Couldn't compare" when no game sound was heard.
- The all-games preset advice and the Fix list agree.
- Mentions the vkcapture Game Capture source when that plugin is installed.
- Settings that are already fine no longer carry fix text (CS2 MSAA).
- Logs keep normal activity again (they only kept warnings since the rename); stopping a test is no longer logged as a warning.
- Viewer preview uses the recording that was already running when you test during a stream (it's never offered for deletion).
- Reports include game settings files in full only for the game you tested or are playing.

**Releases**
- A Release workflow publishes a GitHub Release when a version tag arrives, after checking that the tag, Cargo.toml and download/version.txt match. Installs never depend on it.

## v0.1.0-beta.4

- **Install from the repository**: the finished app now lives in the repo's `download/` folder, so a new version is just an upload. No GitHub Releases, tags or workflows.
- **One line to install**: `curl -fsSL https://raw.githubusercontent.com/Matt-B-007/Stream-Preflight/main/install.sh | sh`.
- **Updates** read `download/version.txt` and check each download against its SHA-256 before installing. Clear messages if GitHub is still catching up after an upload.
- Downloads are gzipped (about 6 MB instead of 14 MB).
- Includes everything from beta.3, which couldn't be installed from GitHub.

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
