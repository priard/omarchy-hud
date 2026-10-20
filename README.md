# omarchy-hud

A Unity-style HUD for [Omarchy](https://omarchy.org): press a key on any window, fuzzy-search
the commands from that app's menus, hit Enter to run one.

It reads menus straight from D-Bus (`org.gtk.Menus`, `org.gtk.Actions`,
`com.canonical.dbusmenu`), finding the focused app's connections by PID, so it needs
no Hyprland plugin and no app changes.

## Status

| Toolkit | Works | Notes |
|---|---|---|
| GTK3/GTK4 apps with a menubar (Xournal++) | yes | full labels and shortcuts |
| GTK apps without a menubar (Nautilus, Evince) | yes | action names turned into labels |
| Qt/KDE (OBS) | partly | needs `QT_QPA_PLATFORMTHEME=generic` and a running AppMenu registrar, see BRIEF.md |
| Electron, Chromium, Firefox, terminals | no | they export no menus on Wayland |

## Install

Requires `python-gobject` and `hyprctl`, both already on Omarchy.

```bash
ln -s "$PWD/omarchy-hud" ~/.local/bin/omarchy-hud
```

Bind it in `~/.config/hypr/bindings.lua` (`SUPER + ALT + SPACE` is taken by Omarchy):

```lua
o.bind("SUPER + ALT + M", "HUD", "omarchy-hud")
```

## Usage

```bash
omarchy-hud                      # menu for the focused window
omarchy-hud --list --pid 1234    # print the commands of a process
omarchy-hud --run "File › Save" --pid 1234
```

## Development

```bash
python3 -m unittest discover -s tests
```

Test against live apps over D-Bus with `--list` and `--run`, never by simulating keystrokes.
For example, toggle a stateful action and read it back:

```bash
./omarchy-hud --pid "$(pgrep -x xournalpp)" --run "View › Layout › Vertical"
gdbus call --session -d <unique-name> -o /com/github/xournalpp/xournalpp/window/1 \
  -m org.gtk.Actions.Describe set-layout-vertical
```

Labels follow the app's language, so use the ones `--list` prints.

## License

MIT
