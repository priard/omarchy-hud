# omarchy-hud

A Unity-style HUD for [Omarchy](https://omarchy.org): press a key on any window, fuzzy-search
the commands from that app's menus, hit Enter to run one.

It reads menus straight from D-Bus (`org.gtk.Menus`, `org.gtk.Actions`,
`com.canonical.dbusmenu`), finding the focused app's connections by PID, so it needs
no Hyprland plugin and no app changes. It is a single Rust binary that lives only while
the menu is open: about 6 ms and 3 MB of RAM to collect a menu of 150 commands.

## Status

| Toolkit | Works | Notes |
|---|---|---|
| GTK3/GTK4 apps with a menubar (Xournal++) | yes | full labels and shortcuts |
| LibreOffice | yes | its GTK3 menubar, per window |
| GTK apps without a menubar (Nautilus, Evince) | yes | action names turned into labels |
| Qt/KDE (OBS) | partly | needs `QT_QPA_PLATFORMTHEME=generic` and a running AppMenu registrar, see BRIEF.md |
| Electron, Chromium, Firefox, terminals | no | they export no menus on Wayland |

## Install

Needs Rust (`pacman -S rust`) to build; at runtime only libdbus, which every Omarchy system has.

```bash
cargo build --release
ln -s "$PWD/target/release/omarchy-hud" ~/.local/bin/omarchy-hud
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
cargo test
cargo run --release --example bench -- 30 "$(pgrep -x xournalpp)" target/release/omarchy-hud
```

Test against live apps over D-Bus with `--list` and `--run`, never by simulating keystrokes.
For example, toggle a stateful action and read it back:

```bash
omarchy-hud --pid "$(pgrep -x xournalpp)" --run "View › Layout › Vertical"
gdbus call --session -d <unique-name> -o /com/github/xournalpp/xournalpp/window/1 \
  -m org.gtk.Actions.Describe set-layout-vertical
```

Labels follow the app's language, so use the ones `--list` prints.

## Performance

0.1.0 was a Python script (PyGObject) that showed its list through `omarchy-menu-select`.
0.2.0 is a Rust rewrite (`dbus` crate over libdbus, no async runtime) measured on 2026-10-22,
Omarchy 4.5.0rc1, Hyprland 0.56.2.

**Collecting the commands** (`--list`, median of 30 runs, from `rusage`, `examples/bench.rs`):

| App (commands) | Python 0.1.0: wall / CPU / peak RAM | Rust 0.2.0: wall / CPU / peak RAM |
|---|---|---|
| Xournal++ (148) | 127 ms / 88 ms / 31.6 MB | 5.8 ms / 3.2 ms / 3.2 MB |
| Nautilus (19) | 79 ms / 77 ms / 31.3 MB | 2.2 ms / 1.6 ms / 3.2 MB |
| Evince (11) | 128 ms / 80 ms / 31.1 MB | 4.4 ms / 2.6 ms / 3.4 MB |
| OBS, Qt (68) | 93 ms / 95 ms / 31.6 MB | 4.0 ms / 2.7 ms / 3.5 MB |

For scale, `/bin/true` peaks at 2.2 MB on the same machine.

**From launch to visible menu** (Xournal++, median of 10, polling `hyprctl layers`):
181 ms in 0.1.0, 27 ms in 0.2.0. Most of what is left is the Omarchy shell opening its
menu (about 20 ms, the same for 5 or 147 rows).

Where the time went, and what fixed it:

| Cost in 0.1.0 | Fix |
|---|---|
| Waiting 500 ms (0.1.0) or 50 ms (after the first fix) for app connections that never answer D-Bus | All connections are probed in one pipelined batch; once one answers, the rest get 2 ms. If that finds nothing, the cut ones get a second, patient try. |
| Python + PyGObject start-up, ~60 ms CPU, 31 MB | Rust binary, 0.8 MB, starts in about 1 ms |
| `omarchy-menu-select`: bash plus three perl runs to build JSON, ~45 ms | The payload is built in-process and sent with one `omarchy-shell` call |
| `hyprctl activewindow` spawn | Hyprland's socket is queried directly |
| One D-Bus round trip per name and per object | `GetConnectionUnixProcessID` and `Introspect` are pipelined, one batch per tree level |

Lazy loading was considered and dropped: fuzzy search needs every command up front,
and fetching all 148 Xournal++ commands takes about a millisecond.

## License

MIT
