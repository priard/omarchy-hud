# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
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
