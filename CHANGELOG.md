# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.3.0] - 2026-10-22

### Fixed
- LibreOffice collection no longer copies its 770 actions once per menu item (23 → 12 ms).

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
