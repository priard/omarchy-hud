# omarchy-hud: project brief (handover from the research session, 2026-10-21)

## Goal
A Ubuntu Unity (2012) style HUD for Omarchy: a key binding on the focused window opens
the Omarchy menu with every command from THAT app's menus, fuzzy search, Enter runs the
command. Target: GTK, GNOME, Qt, KDE and Omarchy's own apps.

## Environment
- Omarchy 4.5.0rc1, Hyprland 0.56.2 (Lua config: ~/.local/share/omarchy/default/hypr/,
  user changes in ~/.config/hypr/)
- Bindings use `o.bind("SUPER + ALT + X", "Description", "command")`
  (e.g. ~/.local/share/omarchy/default/hypr/bindings/utilities.lua)
- `SUPER + ALT + SPACE` is TAKEN ("Apps": `omarchy-shell omarchy.dock apps`, utilities.lua:2).
  Pick another binding (proposal: SUPER + ALT + M, verified free) or override it on purpose.
- omarchy-shell plugins have a limited read-only API, so the logic runs as a separate
  process (CLI) called from a menu or a binding.
- Installed: gtk3, gtk4, qt6-base, kxmlgui, kconfigwidgets, libdbusmenu-glib/gtk3, python3 + gi.
  Test apps: nautilus, xournalpp, evince, obs, libreoffice, pinta, mpv.
- Walker is NOT installed; `omarchy-menu-select` (stdin rows, returns the label) is the launcher.
- Starting an app on a background workspace from a shell:
  `hyprctl dispatch "hl.exec_cmd('[workspace 9 silent] xournalpp')"` (it prints an error, but the app starts).

## Working rules
- No AI attributions in commits/PRs (Co-Authored-By, "Generated with"); global hooks enforce it.
- Do NOT type text or Enter with wtype during UI tests (it once landed in the Claude Code prompt).
- Test menus over D-Bus (gdbus/busctl, `omarchy-hud --list/--run`), not by simulating keystrokes.
- Everything in the repo is in English.

## Research findings (verified live)

### Window → menu: by PID, no Hyprland support needed
- Hyprland does NOT implement the appmenu protocols (no org_kde_kwin_appmenu or gtk_shell1 in the binary).
- Works: `hyprctl activewindow -j` → `.pid`, then the D-Bus connections with that PID
  (`GetConnectionUnixProcessID`) → introspection.
- No Hyprland plugin needed.
- Some connections of an app never answer Introspect (busctl hangs 25 s), so use a short timeout.

### Results per app
| App | Interface | Result |
|---|---|---|
| Xournal++ (GTK3, GtkApplication menubar) | org.gtk.Menus at /com/github/xournalpp/xournalpp/menus/menubar | 198 items with labels (localized), 149 enabled |
| Nautilus (GTK4) | org.gtk.Actions at /org/gnome/Nautilus (app.*) and /org/gnome/Nautilus/window/N (win.*) | ~10 + 14 actions, no labels; Activate works |
| Evince (GTK) | org.gtk.Actions at /org/gnome/Evince/window/1 | ~40 actions, no labels |
| OBS (Qt6), default (QT_QPA_PLATFORMTHEME=gtk3) | only /MenuBar = tray menu (SNI) | no window menu |
| OBS with QT_QPA_PLATFORMTHEME=generic + a registrar on the bus | com.canonical.dbusmenu at /MenuBar/2 | 107 items (File, Edit, View…) |
| Zen (Firefox) | only MPRIS + /org/mozilla/zen/Remote | nothing |
| Mattermost (Electron), Spotify | no window menu (Spotify: tray menu) | nothing |
| foot / TUI | none | nothing |

### Useful commands
```bash
# GTK actions
gdbus call --session -d org.gnome.Nautilus -o /org/gnome/Nautilus/window/1 -m org.gtk.Actions.List
gdbus call --session -d org.gnome.Nautilus -o /org/gnome/Nautilus/window/1 \
  -m org.gtk.Actions.Activate toggle-sidebar "[]" "{}"
# GTK menus (subscribe to group 0, then the groups it references)
gdbus call --session -d <unique> -o <path>/menus/menubar -m org.gtk.Menus.Start "[0]"
# Qt dbusmenu (note the "--" before -1, otherwise gdbus reads -1 as an option)
gdbus call --session -d <unique> -o /MenuBar/2 -m com.canonical.dbusmenu.GetLayout -- 0 -1 "['label']"
# run a dbusmenu item
gdbus call --session -d <unique> -o /MenuBar/2 -m com.canonical.dbusmenu.Event <id> clicked "<0>" 0
```

### Pitfalls
1. Qt exports menus only with some platform themes. `gtk3` (Omarchy default) does NOT,
   `generic` does, but changes how Qt apps look. UNVERIFIED: whether `kde`/plasma-integration
   exports menus and keeps the look. This is the main decision for Qt/KDE.
2. Qt on Wayland never calls com.canonical.AppMenu.Registrar.RegisterWindow, BUT it exports
   the menubar only while the name com.canonical.AppMenu.Registrar is owned on the bus
   (verified 2026-10-21: OBS with `generic` and no registrar exports only the tray menu).
   So Qt support needs a tiny registrar service that just owns the name (~20 lines with Gio,
   methods RegisterWindow/UnregisterWindow/GetMenuForWindow as no-ops). Mapping stays by PID.
3. GTK4 does not export hamburger menus, only actions (names, no labels).
   Labels come from names ("toggle-sidebar" → "Toggle sidebar"); when org.gtk.Menus exists, its labels win.
4. Several windows of one app (/window/1, /window/2) can't be matched to Hyprland's focused window.
   MVP: app actions + the first window; heuristics later.
5. Dynamic dbusmenu menus (recent files etc.) need AboutToShow before GetLayout.
6. Electron, Chromium, Firefox/Zen and terminals are out of reach (no menu export on Wayland).
7. Moving PIDs: Electron and some apps hold D-Bus connections in child processes,
   so check the PID's children too. Note: `setsid app & echo $!` does NOT give the app's PID.

## Architecture (MVP in 0.1.0 in Python, rewritten in Rust in 0.2.0)
CLI `omarchy-hud` (Rust, `dbus` crate over libdbus):
1. Focused window's PID (hyprctl), D-Bus connections of the process and its children.
2. Collects items from three sources: org.gtk.Menus (labels + action), org.gtk.Actions (app./win.),
   com.canonical.dbusmenu (Qt/KDE). Output: a flat list "File › Export as PDF", enabled/visible only.
3. Shows it in the Omarchy menu (the `omarchy.menu` select payload via `omarchy-shell`, built-in fuzzy search).
4. Selection → org.gtk.Actions.Activate or dbusmenu Event "clicked".
5. Binding in ~/.config/hypr (not in default/).

Omarchy apps (Qt Quick, no QMenuBar): a convention that the app exports org.gtk.Actions
or dbusmenu with its commands, so the HUD handles them with no extra code.

## Work order
1. ~~GTK MVP (Xournal++ = org.gtk.Menus, Nautilus/Evince = org.gtk.Actions) + D-Bus test.~~ Done in 0.1.0.
2. ~~Launcher choice~~ (`omarchy-menu-select`) and binding (open).
3. Qt: decide QT_QPA_PLATFORMTHEME (check plasma-integration), ship the registrar service.
4. Labels for GTK4 actions, multiple windows, dynamic menus.
5. Convention for Omarchy apps.
6. Unverified: LibreOffice, native KDE apps (none installed).

## Decisions for the user
- Binding (SUPER+ALT+SPACE is taken; SUPER+ALT+M is free).
- Whether to change the Qt theme for all apps.
- Long term: a separate AUR package or a PR to Omarchy.
