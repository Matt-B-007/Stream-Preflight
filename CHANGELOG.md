# Changelog

## v0.1.0-beta.10

A round of fixes: 51 bugs found by going through every screen, button and message in beta.9. No new features.

- **The tour now appears on new installs.** It was waiting for a setup step that no longer exists.
- **No false "closed unexpectedly" notice.** Logging out, shutting down or `kill` now count as a normal close.
- **Dialogs are real dialogs.** Report a problem, Update and Uninstall dim the window behind them; nothing behind reacts, Escape closes them, and they scroll instead of growing taller than the window.
- **The "test recording is still saved" reminder waits** until you're not live, not testing and no other dialog is open.
- **Command line**: `--help` lists the options; an unknown option is an error (exit code 2) instead of opening the window; `--uninstall` asks first (`--yes` skips the question); the exit code is 1 when the window can't open; a full `--self-test` no longer fails just because OBS is closed.
- **Reports hide more, and garble less**: version numbers after a program name ("obs 31.0.2.1") stay; email addresses become `<email>`; user names with a space are removed whole; a home folder inside a longer name is no longer cut in half; a PC named like the distro (for example "cachyos") no longer turns "CachyOS" into `<host>`; code paths like `a::b` and ratios are no longer taken for IPv6 addresses; `aabb.ccdd.eeff` MAC addresses are removed.
- **DNS fallback has a switch and is in the privacy notes**: **Settings › Updates › If this PC can't find GitHub, ask a public DNS service**.
- **The numbers agree**: Home, the sidebar badge and Fixes show the same "to fix" count, with optional ones named separately; the Setup score is blank until OBS has been read; a stream under a minute shows seconds instead of "0 min"; History no longer says "Nothing to change" above a tile that shows lost frames; Live says "not readable" instead of "has room" or "busy" when there's no reading; the OBS line no longer has a stray comma.
- **The Test page says where it is**: "Test in progress", then "Test complete", and it says when no game was running.
- **Wording brought up to date**: the tray icon turns lime when live (not red); "stream summary" instead of "post-flight"; logs are "1 MB per file, the last 3 kept"; status words in History are Good / Check / Fix.
- **What's new is in plain words** and opens once by itself after an update. Help no longer carries years of old release notes.
- **Layout**: cards no longer run under the scrollbar; Undo and other notices sit above the Next bar; small buttons ("Suggest again", "Open", "Close") are full height; the Next bar's sentence shortens instead of running under its button; in a narrow window "Not live" becomes a dot instead of overlapping the chips; the style menu's text is no longer stretched; the Last stream numbers on Home no longer touch.
- **Top bar and sidebar**: chips that jump to a page no longer show a menu arrow; the setup screen has Help and Settings top right too; Settings and Help are top right only (Help is the first item in the ? menu), not also in the sidebar.
- **Settings**: Advanced now has the self-test, "Record more detail" and the reports folder (moved from Help), with Install; "Background grid" is greyed while Reduce motion is on; "Minimize to tray" says "Minimize" on a desktop without a tray.
- **Report a problem**: the saved path wraps and can be selected; all buttons sit in one row.
- **Lighter when idle**: Fixes redraws every 5 seconds instead of every second, and Help every 15 instead of every 3.
- **Tray**: the icon now changes on time when the window is hidden, and in tray-only mode (where the 400 ms hold could stop it changing at all).
- **Undo isn't cut short** by another notice arriving, and helper programs (file manager, text editor) no longer leave a finished process behind.
- **`download/` stays at beta.8**, frozen through this release; it's deleted in the next one.

## v0.1.0-beta.9

The family look and flow: a light theme, the same layout and dialogs as the other apps, and safer settings, logs and reports. First version that only installs signed updates.

- **Light and dark themes**: follows your desktop by default, or pick Dark or Light in **Settings › Appearance**. The navy "hull" colours shared by the family (house colour update v1.3), lime text and links (#3F6212 in the light theme), and the same green, amber and red for good, check and fix everywhere. Every colour comes from one place, and a test checks that all text is readable (4.5:1) on every background in both themes; `--self-test` checks it too.
- **Calmer effects**: the purple glow is gone and only the main button glows (a soft 12 px lime glow). Corner brackets mark only the active card; other cards get a thin lime edge. The background grid can be turned off, and **Reduce motion** turns off animations.
- **Built-in fonts and icons**: Inter for text, JetBrains Mono for numbers and logs, Space Grotesk for titles, and Lucide icons, all inside the program, so text looks the same on every PC. Text sizes are 12 / 14 / 16 / 22. Licences are in `assets/fonts`.
- **New app icon and tray states**: the launcher icon is the lime symbol on #0C1016. The tray icon is grey when OBS isn't connected, plain when ready, lime while testing or live, with an amber dot when something needs you and a red dot when something is wrong. It waits 400 ms before changing, so it doesn't flicker.
- **Help menu, top right**: Report a problem, What's new, Keyboard shortcuts, Show the tour and About, next to the Settings gear and an Update button when there's an update.
- **Update dialog**: "Stream Preflight X is available" with the release notes, **Update now**, **Later** and **Skip this version**, then "Update installed. Restart to use it." Preflight no longer restarts by itself. **Settings › Updates** has a Stable / Beta switch and says when it last checked.
- **Report a problem**: a short summary, "What happened, and what did you expect?", and a preview of exactly what's included. **Save to file**, **Copy report**, **Open log folder**, and **Open on GitHub**, which warns that it creates a public issue and fills in only the summary and the app and system lines. The report starts with app, version, channel, install type, distro, desktop, Wayland/X11 and kernel.
- **Settings in the family order**: General, Appearance, Updates, Privacy, OBS & PCs, Tray & notifications, Advanced, About & uninstall, with the red Uninstall box last. Uninstalling now keeps your settings and logs unless you tick **Also delete my settings and logs**, and ends with "Stream Preflight is uninstalled. Thanks for trying it."
- **Keyboard**: Ctrl+K searches pages and commands, Ctrl+, opens Settings, ? shows all shortcuts, Ctrl+Q quits.
- **A short tour** for new installs (5 steps, skippable). After an update it's offered in What's new instead.
- **Undo**: clearing history, forgetting the OBS password, unpairing and deleting a test recording all offer **Undo** for 10 seconds before anything is removed for good.
- **"Closed unexpectedly" notice**: if Preflight didn't close normally last time, it says so at the next start, with **Report** and **Dismiss**. Crash files from the last 10 sessions are kept.
- **Safer settings**: saved through a temporary file and a rename, with a rolling backup at most every 15 minutes (5 kept). A broken settings file is replaced by the newest good backup instead of starting fresh. History and other data files are saved the same way.
- **New folders on Linux**: `~/.config/stream-preflight` (settings), `~/.local/share/stream-preflight` (history and data) and `~/.local/state/stream-preflight` (logs), moved automatically from the old folders. The old folder names become links to the new ones, so going back to an older version still finds everything.
- **Logs**: up to 1 MB per file, 3 files kept, plus the last 500 lines in memory for reports.
- **Reports remove more**: the same placeholders as the other apps: `~`, `<user>`, `<host>`, `<ip>` (IPv4 and IPv6), `<mac>`, `<secret>` and `<steamid>`, including Windows-style paths like `Z:\home\name`. Version numbers such as 0.11.0.1 are no longer mistaken for addresses.
- **Updates when DNS fails**: if your PC's own DNS can't find GitHub, the updater asks a public DNS service (Cloudflare, then Google) over HTTPS. The connection to GitHub is still checked as usual.
- **Signed updates only**: from this version on, an update installs only with a valid signature from the maintainer's main or backup key.
- **`download/` stays at beta.8**: it's no longer updated. It stays frozen through this release and the next, then it's deleted.

## v0.1.0-beta.8

Safer updates: signed GitHub Releases, a self-test before every swap, and a way to go back. Plus three fixes from the latest bug report.

- **Signed releases**: each release is built by GitHub Actions from its tag, as a draft first. `SHA256SUMS` is signed with minisign and checked against both public keys before the release is published. The app installs an update only when the download matches `SHA256SUMS` and the signature comes from the main or the backup key; a release without its signature yet counts as "not ready yet". This version is installed by the old updater; from here on, signatures are required.
- **Self-test before every swap**: an update is put next to the program, must pass `--self-test --quick`, and only then replaces it.
- **Go back**: the version you had is kept. **Settings › Updates & about › Go back to vX**, or `stream-preflight --rollback`. Installs in `/usr` or `/opt` point you to your package manager instead.
- **One window at a time**: opening Preflight again brings the open window to the front, instead of starting a second copy. Updates, the tray hand-over and the installer still restart it cleanly.
- **Command line**: `--version`, `--self-test [--quick]`, `--report`, `--update`, `--rollback` and `--uninstall [--delete-settings]`.
- **Installer options**: `--beta`, `--version X`, `--no-launch`, `--from-source` and `--uninstall`. It reads GitHub Releases, checks the checksum, and checks the signature when minisign is installed. While there's no signed release yet, it falls back to the `download/` folder.
- **Upload test reused for 6 hours**, kept across restarts, so a quick re-check doesn't use data every time. The Network card says how old the result is.
- **Game audio shows Unknown** when OBS isn't connected, instead of "None" marked Good.
- **One wording for the NVENC preset** in the Setup table and the fleet line: "P6 or P7", "P6 or P7 (P5 for <busy game>)", or "P5" when every tested game keeps the GPU busy.
- **Licence**: GPL-3.0-or-later, with SPDX headers in every source file and a dependency licence check (`cargo deny check licenses`) in `scripts/check.sh`. `check.sh` also runs the program's quick self-test and checks the issue templates.
- **Repository docs**: CONTRIBUTING, SUPPORT, SECURITY and RELEASING, plus a bug report form and a pull request checklist.
- **Last version in `download/`**: beta.8 is placed there one last time, so beta.4 to beta.7 can update to it. `download/` then stays frozen and will be deleted after two more releases.

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
