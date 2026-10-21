//! A minimal com.canonical.AppMenu.Registrar. Qt exports its menubar over dbusmenu only
//! while this name is owned when a window is created; on Wayland it never registers
//! windows, so the HUD still finds menus by PID. X11 apps that do register are tracked
//! so GetMenuForWindow answers for them.

use std::collections::HashMap;
use std::ffi::CString;

use dbus::channel::{BusType, Channel};
use dbus::message::MessageType;
use dbus::strings::{ErrorName, Path};
use dbus::Message;

const NAME: &str = "com.canonical.AppMenu.Registrar";
const PATH: &str = "/com/canonical/AppMenu/Registrar";
const INTROSPECTION: &str = r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<node>
  <interface name="com.canonical.AppMenu.Registrar">
    <method name="RegisterWindow"><arg name="windowId" type="u" direction="in"/><arg name="menuObjectPath" type="o" direction="in"/></method>
    <method name="UnregisterWindow"><arg name="windowId" type="u" direction="in"/></method>
    <method name="GetMenuForWindow"><arg name="windowId" type="u" direction="in"/><arg name="service" type="s" direction="out"/><arg name="menuObjectPath" type="o" direction="out"/></method>
  </interface>
  <interface name="org.freedesktop.DBus.Introspectable">
    <method name="Introspect"><arg name="xml" type="s" direction="out"/></method>
  </interface>
</node>"#;

pub fn run() -> Result<(), String> {
    let channel = Channel::get_private(BusType::Session).map_err(|e| format!("session bus: {e}"))?;
    // DO_NOT_QUEUE: if another registrar (e.g. KDE's) runs, leave it be.
    let request = Message::new_method_call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "RequestName")?
        .append2(NAME, 4u32);
    let reply = channel.send_with_reply_and_block(request, std::time::Duration::from_secs(5)).map_err(|e| e.to_string())?;
    if reply.read1::<u32>() != Ok(1) {
        return Ok(());
    }

    let mut windows: HashMap<u32, (String, Path<'static>)> = HashMap::new();
    loop {
        channel.read_write(None).map_err(|_| "lost the session bus")?;
        while let Some(msg) = channel.pop_message() {
            if msg.msg_type() != MessageType::MethodCall {
                continue;
            }
            let reply = answer(&msg, &mut windows);
            let _ = channel.send(reply);
        }
        channel.flush();
    }
}

fn answer(msg: &Message, windows: &mut HashMap<u32, (String, Path<'static>)>) -> Message {
    let member = msg.member().map(|m| m.to_string()).unwrap_or_default();
    let on_registrar = msg.path().is_some_and(|p| &*p == PATH);
    match member.as_str() {
        "Introspect" if on_registrar => msg.method_return().append1(INTROSPECTION),
        "Ping" => msg.method_return(),
        "RegisterWindow" if on_registrar => match (msg.get2::<u32, Path>(), msg.sender()) {
            ((Some(id), Some(path)), Some(sender)) => {
                windows.insert(id, (sender.to_string(), path.into_static()));
                msg.method_return()
            }
            _ => error(msg, "org.freedesktop.DBus.Error.InvalidArgs", "expected (uo)"),
        },
        "UnregisterWindow" if on_registrar => {
            if let Some(id) = msg.get1::<u32>() {
                windows.remove(&id);
            }
            msg.method_return()
        }
        "GetMenuForWindow" if on_registrar => match msg.get1::<u32>().and_then(|id| windows.get(&id)) {
            Some((service, path)) => msg.method_return().append2(service.as_str(), path.clone()),
            None => error(msg, "com.canonical.AppMenu.Registrar.Error.NotFound", "no menu for this window"),
        },
        _ => error(msg, "org.freedesktop.DBus.Error.UnknownMethod", "unknown method"),
    }
}

fn error(msg: &Message, name: &'static str, text: &str) -> Message {
    msg.error(&ErrorName::from(name), &CString::new(text).unwrap_or_default())
}
