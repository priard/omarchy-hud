"""Unit tests for the pure parsing helpers. Live D-Bus checks are in the README."""

import importlib.machinery
import importlib.util
import pathlib
import unittest

_path = str(pathlib.Path(__file__).resolve().parent.parent / "omarchy-hud")
_loader = importlib.machinery.SourceFileLoader("omarchy_hud", _path)
_spec = importlib.util.spec_from_loader("omarchy_hud", _loader)
hud = importlib.util.module_from_spec(_spec)
_loader.exec_module(hud)


class Labels(unittest.TestCase):
    def test_clean(self):
        self.assertEqual(hud.clean("_File"), "File")
        self.assertEqual(hud.clean("Pasek _stanu"), "Pasek stanu")
        self.assertEqual(hud.clean("snake__case"), "snake_case")

    def test_humanize(self):
        self.assertEqual(hud.humanize("toggle-sidebar"), "Toggle sidebar")

    def test_gtk_accel(self):
        self.assertEqual(hud.gtk_accel("<Primary><Shift>s"), "Ctrl+Shift+S")
        self.assertEqual(hud.gtk_accel("F11"), "F11")
        self.assertEqual(hud.gtk_accel(""), "")


class GtkMenu(unittest.TestCase):
    def test_walk_sections_and_submenus(self):
        menus = {
            (0, 0): [{"label": "_File", ":submenu": (1, 0)}],
            (1, 0): [{":section": (1, 1)}, {"label": "_Quit", "action": "app.quit", "accel": "<Primary>q"}],
            (1, 1): [{"label": "_Layout", "action": "win.layout", "target": "grid"}],
        }
        self.assertEqual(list(hud.walk_gtk_menu(menus, (0, 0))), [
            ("File › Layout", "", "win.layout", "grid"),
            ("File › Quit", "<Primary>q", "app.quit", None),
        ])


class DbusMenu(unittest.TestCase):
    def test_walk_skips_hidden_disabled_and_separators(self):
        root = (0, {}, [
            (1, {"label": "_File"}, [
                (2, {"label": "_Save", "shortcut": [["Control", "S"]]}, []),
                (3, {"type": "separator"}, []),
                (4, {"label": "Hidden", "visible": False}, []),
                (5, {"label": "Off", "enabled": False}, []),
            ]),
        ])
        self.assertEqual(list(hud.walk_dbusmenu(root)), [("File › Save", "Ctrl+S", 2)])


if __name__ == "__main__":
    unittest.main()
