# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- Pack shortcuts show punctuation as the key itself: `Ctrl+/` instead of `Ctrl+slash`.
- README screenshot: the HUD over RAWmakase.

## [0.5.0] - 2026-10-22

### Added
- The menu opens over the focused window instead of the middle of the screen, with
  `dist/menu/install`: a copy of Omarchy's menu plugin (`omarchy-hud.menu`) plus
  `menu-anchor.patch`, which centers the menu on a rectangle the HUD sends. Only the HUD
  summons it; without it the HUD falls back to Omarchy's menu. The patch is proposed upstream.
- `io.github.priard.OmarchyHud.Commands1`, a two-method D-Bus interface (`List`, `Activate`)
  for apps without a menu model, with a GDBus example in `docs/examples/`.
- Shortcut packs installed by apps in `$XDG_DATA_HOME` and `$XDG_DATA_DIRS`
  (`omarchy-hud/shortcuts/*.conf`), between the user's packs and the built-in ones.
- A built-in pack for RAWmakase (74 commands, from its shortcut reference).
- `docs/integration.md`: every way an app can offer its commands to the HUD.
- README: a screenshot, where the idea comes from, and a proof-of-concept notice.

## [0.4.0] - 2026-10-22

### Added
- MPRIS playback controls (Play/Pause, Next track, Previous track, Stop) for any app that
  owns an `org.mpris.MediaPlayer2.*` name, shown only when the player says it can do them.
  Spotify had only its tray menu before.
- Shortcut packs: commands for apps that export no menu, run by sending the app's own key
  chord to its window through Hyprland. Built-in packs for Firefox and its forks (with Zen's
  own chords), Chromium-based browsers, Thunderbird and Flea, taken from the apps' sources
  where they ship them. User packs in `~/.config/omarchy-hud/shortcuts/*.conf` add commands
  or override built-in ones by label.
- The bench example takes a window address as well as a pid.

### Changed
- Running a pack command focuses its window first, so `--run` with `--window` now focuses
  that window.

## [0.3.0] - 2026-10-22

### Added
- Apps with several windows: the HUD picks the focused one by matching creation order
  (Hyprland's `stableID` against the app's window numbers, floating dialogs left out),
  and falls back to the first window when the counts differ. Works for LibreOffice's
  per-window menubars and GTK's `/window/N` actions.
- `--window ADDRESS` to act on a given Hyprland window without focusing it.
- Labels for GTK4 apps without a menubar come from the GtkBuilder files compiled into
  the app and its gettext catalog, so Nautilus shows "Nowa karta" instead of "New tab".
  Parameterized actions from those menus work too.
- Recently picked commands come first, per app (10 each, in
  `$XDG_STATE_HOME/omarchy-hud/recent`). Only menu picks count, not `--run`.
- `omarchy-hud registrar` and `dist/omarchy-hud-registrar.service`: a tiny AppMenu registrar
  so Qt and KDE apps export their menus (OBS: 68 commands, Kdenlive: 630). Opt-in, because
  Qt then hides the in-window menubar.
- LibreOffice: its per-window menubars now work (516 commands in Writer); each menubar is
  matched with its own window's actions.

### Fixed
- LibreOffice collection no longer copies its 770 actions once per menu item (23 → 12 ms).

## [0.2.0] - 2026-10-22

### Changed
- Rewritten in Rust: the menu shows up in 27 ms instead of 181 ms, collecting commands
  takes 2–6 ms instead of 80–130 ms and peaks at 3 MB of RAM instead of 31 MB.
  See Performance in the README.
- The menu payload goes straight to `omarchy-shell` instead of through `omarchy-menu-select`.
- The focused window comes from Hyprland's socket instead of `hyprctl`.

### Fixed
- Opening the HUD no longer waits for app connections that never answer D-Bus.

## [0.1.0] - 2026-10-21

### Added
- `omarchy-hud` CLI: finds the focused window's D-Bus connections by PID (including child
  processes) and lists its commands in the Omarchy menu (`omarchy-menu-select`).
- GTK menubars via `org.gtk.Menus`, with labels, shortcuts and targeted actions.
- GTK actions via `org.gtk.Actions` for apps without a menubar, with labels made from action names.
- Qt/KDE menus via `com.canonical.dbusmenu`, including dynamic submenus.
- `--list`, `--run` and `--pid` options for scripting and testing.
