#!/usr/bin/env python3
"""A minimal app exporting io.github.priard.OmarchyHud.Commands1, for docs/integration.md.

Run it, then: omarchy-hud --pid <its pid> --list
"""

import os

from gi.repository import Gio, GLib

INTERFACE = """
<node>
  <interface name="io.github.priard.OmarchyHud.Commands1">
    <method name="List">
      <arg name="commands" type="a(sss)" direction="out"/>
    </method>
    <method name="Activate">
      <arg name="id" type="s" direction="in"/>
    </method>
  </interface>
</node>
"""

COMMANDS = [
    ("new", "File › New note", "Ctrl+N"),
    ("export-pdf", "File › Export as PDF", ""),
    ("zen", "View › Zen mode", "F11"),
]


def on_call(connection, sender, path, interface, method, params, invocation):
    if method == "List":
        invocation.return_value(GLib.Variant("(a(sss))", (COMMANDS,)))
    elif method == "Activate":
        (command_id,) = params.unpack()
        print("activated", command_id, flush=True)
        invocation.return_value(None)


def on_bus(connection, name):
    info = Gio.DBusNodeInfo.new_for_xml(INTERFACE).interfaces[0]
    connection.register_object("/io/github/priard/OmarchyHud", info, on_call, None, None)
    print("pid", os.getpid(), "on", connection.get_unique_name(), flush=True)


Gio.bus_own_name(Gio.BusType.SESSION, "org.example.NotesApp", Gio.BusNameOwnerFlags.NONE, on_bus, None, None)
GLib.MainLoop().run()
