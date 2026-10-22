# Making an app work with the HUD

The HUD finds an app's commands by itself. When the user presses the HUD key, it takes the focused window's process ID from Hyprland, finds that process's D-Bus connections and those of its child processes, introspects them, and reads every command source below that it finds. Nothing is registered and nothing runs in the background.

This page is for app developers. It lists every way an app can offer its commands, from "you probably already do" to "five minutes of work", and what each one costs.

| Your app | Use | Work |
|---|---|---|
| GTK 3/4 with `GtkApplication` menus | [GTK menus and actions](#1-gtk-menus-and-actions) | none, or name your menus |
| Qt or KDE with a `QMenuBar` | [dbusmenu through the registrar](#2-qt-and-kde-dbusmenu) | none |
| Any toolkit, with a menu model | [`com.canonical.dbusmenu`](#3-comcanonicaldbusmenu) | a library call |
| Any toolkit, without menus (Qt Quick, web, games, TUIs with a D-Bus connection) | [the HUD's command list](#4-the-huds-command-list) | two D-Bus methods |
| Anything with keyboard shortcuts | [a shortcut pack file](#5-a-shortcut-pack-file) | one text file |
| A media player | [MPRIS](#6-mpris) | none if you already have it |

Sources add up: an app can export a menubar and a command list, and the user sees both, with duplicate labels shown once.

## What every source shares

- **Labels** are shown as given, so translate them yourself. Use ` › ` (space, U+203A, space) to show where a command lives, like `File › Export › PDF`. The menu-based sources build that path from the menu tree.
- **Accelerators** are display text, like `Ctrl+Shift+S`. The HUD does not press them, except for shortcut packs.
- **Speed.** The HUD collects commands every time it opens, so answer in milliseconds. Each connection gets 100 ms to answer the first `Introspect`, and once one of the app's connections has answered the others get 2 ms more. A connection that misses that gets one more try with 500 ms, but only if nothing else turned up any commands.
- **Activation** happens after the HUD's menu has closed and focus is back on your window, so act on your focused window, as a menu click would.

## 1. GTK menus and actions

If your app uses `GtkApplication` and its window is a `GtkApplicationWindow`, GTK already exports everything the HUD reads:

- `org.gtk.Menus` at `/org/gtk/.../menus/menubar` when you call `gtk_application_set_menubar()`. The HUD takes labels, submenus, sections, accelerators and action targets from it. This is the best case.
- `org.gtk.Actions` at your application path (`app.*`) and at `/window/N` (`win.*`), always. Without a menubar the HUD lists these actions, labeled from the menus compiled into your binary (GtkBuilder `<menu>` elements in your GResource, translated with your gettext domain). Actions that appear in no menu get a label made from their name (`toggle-sidebar` becomes "Toggle sidebar").

To get good labels without a menubar, keep your menus (popovers, primary menu, context menus) as `<menu>` definitions in `.ui` files in your GResource, with `translatable="yes"` labels. Disabled actions are left out.

With several windows, the HUD matches the focused window to `/window/N` by creation order.

## 2. Qt and KDE: dbusmenu

Qt exports a `QMenuBar` over `com.canonical.dbusmenu` on its own, but only when a `com.canonical.AppMenu.Registrar` is on the bus as the window opens. The HUD ships one (`omarchy-hud registrar`), which the user turns on; see the README. Nothing to do in your app.

The cost is on the user's side: with a registrar running, Qt hides the menubar inside the window. If you don't want that for your app, use [the command list](#4-the-huds-command-list) instead.

## 3. com.canonical.dbusmenu

Any app can export a menu with `com.canonical.dbusmenu` at any object path, for example with libdbusmenu (`dbusmenu_server_new()` from libdbusmenu-glib) or by implementing the interface yourself. The HUD calls `GetLayout(0, -1, [])`, then `AboutToShowGroup` for every submenu and `GetLayout` again to pick up submenus filled on demand. It runs an item with `Event(id, "clicked", <0>, 0)`.

It reads the `label` (with `_` mnemonics), `visible`, `enabled` (hidden and disabled items are skipped), `type` (separators are skipped) and `shortcut` properties.

## 4. The HUD's command list

For apps without a menu model, a flat list over D-Bus is the least work. Export this interface on any object path, on any connection your process (or a child process) owns:

```xml
<interface name="io.github.priard.OmarchyHud.Commands1">
  <!-- Every command the user can run right now: (id, label, accelerator). -->
  <method name="List">
    <arg name="commands" type="a(sss)" direction="out"/>
  </method>
  <!-- Run the command with this id. -->
  <method name="Activate">
    <arg name="id" type="s" direction="in"/>
  </method>
</interface>
```

- `List` returns every command that makes sense now, in the order to show them. Leave out what can't run in the current state rather than returning it and failing. Commands with an empty label are skipped.
- `id` is yours. The HUD only passes it back to `Activate`.
- `label` and `accelerator` follow [the shared rules](#what-every-source-shares). Use `""` for no accelerator.
- `Activate` should return once the command has started, not when it has finished, because the HUD waits up to 500 ms for the reply. Return an error if the id is unknown.
- No well-known name is needed: the HUD finds you by process ID. Your object has to show up in introspection from `/`, which it does when you register it with GDBus, QtDBus or sd-bus.

A complete example in Python with GDBus is in [examples/commands-server.py](examples/commands-server.py). Run it, then `omarchy-hud --pid <its pid> --list`.

In Qt, the same is a `QDBusAbstractAdaptor` with `Q_CLASSINFO("D-Bus Interface", "io.github.priard.OmarchyHud.Commands1")`, two slots, and `QDBusConnection::sessionBus().registerObject()`. A Qt Quick app can fill the list from the `Action` and `Shortcut` items it already has.

The interface name sits under this project's namespace because nothing like it exists in Omarchy. If Omarchy adopts the idea, the HUD will read the new name as well.

## 5. A shortcut pack file

If your app has keyboard shortcuts, a text file is enough, with no code and no D-Bus. Install it as `/usr/share/omarchy-hud/shortcuts/<your-app>.conf`. The HUD looks in `omarchy-hud/shortcuts/` under `$XDG_CONFIG_HOME` (the user's own), `$XDG_DATA_HOME` and every `$XDG_DATA_DIRS` entry, in that order, and then at its built-in packs. The first line with a given label wins.

```ini
# Sections name the window classes the lines apply to, as in hyprctl clients.
[com.example.Notes notes]
New note = Ctrl+N
Search notes = Ctrl+K
Toggle sidebar = Ctrl+Backslash
Zen mode = F11
```

- Lines are `Label = Chord`. A chord is `Ctrl`, `Shift`, `Alt` and `Super` joined with `+`, then a key by its xkb name (`T`, `F5`, `Delete`, `Page_Down`, `equal`, `comma`, `period`, `bracketright`). Lines that don't parse are skipped.
- A label can use ` › ` like any other.
- When a pack command is picked, the HUD focuses the window and sends the chord to it through Hyprland (`hl.dsp.send_shortcut`). Your app receives a normal key press.
- A pack only works for the chord's current meaning, so keep it in step with your keymap. If users can remap keys, prefer [the command list](#4-the-huds-command-list).

The built-in packs in [`packs/`](../packs) are examples. Most of them were checked against the app's own source (Firefox's `browser-sets.inc`, Zen's `ZenKeyboardShortcuts.mjs`, Thunderbird's `messenger.xhtml`).

## 6. MPRIS

Players that own an `org.mpris.MediaPlayer2.*` name from the window's process get Play/Pause, Next track, Previous track and Stop, each shown only when the player's `CanPause`/`CanPlay`, `CanGoNext`, `CanGoPrevious` and `CanControl` properties allow it. They are calls on `org.mpris.MediaPlayer2.Player` at `/org/mpris/MediaPlayer2`. If you implement MPRIS, there is nothing more to do.

## Testing your integration

```bash
omarchy-hud --list --pid "$(pgrep -x your-app)"           # D-Bus sources
omarchy-hud --list --window 0x55e5787d28f0                # plus shortcut packs, by window
omarchy-hud --run "File › New note" --pid "$(pgrep -x your-app)"
```

`busctl --user introspect <your unique name> /your/path` shows what the HUD sees.
