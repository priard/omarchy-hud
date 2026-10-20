# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] - 2026-10-21

### Added
- `omarchy-hud` CLI: finds the focused window's D-Bus connections by PID (including child
  processes) and lists its commands in the Omarchy menu (`omarchy-menu-select`).
- GTK menubars via `org.gtk.Menus`, with labels, shortcuts and targeted actions.
- GTK actions via `org.gtk.Actions` for apps without a menubar, with labels made from action names.
- Qt/KDE menus via `com.canonical.dbusmenu`, including dynamic submenus.
- `--list`, `--run` and `--pid` options for scripting and testing.
