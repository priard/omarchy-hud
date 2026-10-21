//! Finds an app's menus on D-Bus by PID and flattens them into commands.
//! Sources: org.gtk.Menus (GTK menubars), org.gtk.Actions (GTK without a menubar)
//! and com.canonical.dbusmenu (Qt/KDE).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::time::Duration;

use dbus::arg::{ArgType, PropMap, RefArg, Variant};
use dbus::Signature;

use crate::bus::{self, Bus, TIMEOUT};

const SEP: &str = " › ";
const GTK_MENUS: &str = "org.gtk.Menus";
const GTK_ACTIONS: &str = "org.gtk.Actions";
const DBUSMENU: &str = "com.canonical.dbusmenu";
const DBUS: &str = "org.freedesktop.DBus";
const INTROSPECTABLE: &str = "org.freedesktop.DBus.Introspectable";
// Some connections of an app (worker threads, portals) never answer; once one live
// connection has, the others get this much longer. When that leaves no commands,
// the cut-off connections get a second, patient try.
const PROBE_GRACE: Duration = Duration::from_millis(2);
const PROBE_CAP: Duration = Duration::from_millis(100);
// Guards against menus that reference themselves.
const MAX_DEPTH: usize = 16;

pub struct Command {
    pub label: String,
    pub accel: String,
    pub action: Action,
}

pub enum Action {
    Gtk { name: String, path: String, action: String, target: Option<Box<dyn RefArg>> },
    DbusMenu { name: String, path: String, id: i32 },
}

pub fn activate(bus: &Bus, action: &Action) -> Result<(), String> {
    match action {
        Action::Gtk { name, path, action, target } => {
            let params: Vec<Variant<Box<dyn RefArg>>> = target.iter().map(|t| Variant(t.box_clone())).collect();
            bus.call(name, path, GTK_ACTIONS, "Activate", (action.as_str(), params, PropMap::new()))?;
        }
        Action::DbusMenu { name, path, id } => {
            bus.call(name, path, DBUSMENU, "Event", (*id, "clicked", Variant(0i32), 0u32))?;
        }
    }
    Ok(())
}

pub fn collect(bus: &Bus, pid: u32) -> Vec<Command> {
    let trees = object_trees(bus, &connections(bus, &pid_tree(pid)), Some(PROBE_GRACE));
    let silent: Vec<String> = trees.iter().filter(|(_, objects)| objects.is_empty()).map(|(n, _)| n.clone()).collect();
    let commands = commands_from(bus, trees);
    if commands.is_empty() && !silent.is_empty() {
        return commands_from(bus, object_trees(bus, &silent, None));
    }
    commands
}

fn commands_from(bus: &Bus, trees: Vec<(String, Objects)>) -> Vec<Command> {
    let mut commands = Vec::new();
    let mut seen = HashSet::new();
    for (name, objects) in trees {
        let mut found = Vec::new();
        if objects.values().any(|ifs| ifs.iter().any(|i| i == GTK_ACTIONS || i == GTK_MENUS)) {
            found.extend(gtk_commands(bus, &name, &objects));
        }
        for path in objects.iter().filter(|(_, ifs)| ifs.iter().any(|i| i == DBUSMENU)).map(|(p, _)| p) {
            found.extend(dbusmenu_commands(bus, &name, path));
        }
        commands.extend(found.into_iter().filter(|c| seen.insert(c.label.clone())));
    }
    commands
}

/// The pid and all its descendants (Electron and friends talk D-Bus from children).
fn pid_tree(pid: u32) -> HashSet<u32> {
    let mut tree = HashSet::new();
    let mut todo = vec![pid];
    while let Some(p) = todo.pop() {
        if !tree.insert(p) {
            continue;
        }
        let Ok(tasks) = fs::read_dir(format!("/proc/{p}/task")) else { continue };
        for task in tasks.flatten() {
            if let Ok(children) = fs::read_to_string(task.path().join("children")) {
                todo.extend(children.split_whitespace().filter_map(|c| c.parse::<u32>().ok()));
            }
        }
    }
    tree
}

/// Unique bus names owned by any of the pids.
fn connections(bus: &Bus, pids: &HashSet<u32>) -> Vec<String> {
    let Ok(reply) = bus.call(DBUS, "/org/freedesktop/DBus", DBUS, "ListNames", ()) else { return vec![] };
    let names: Vec<String> = reply.read1::<Vec<String>>().unwrap_or_default().into_iter().filter(|n| n.starts_with(':')).collect();
    let msgs = names
        .iter()
        .filter_map(|n| bus::method_call(DBUS, "/org/freedesktop/DBus", DBUS, "GetConnectionUnixProcessID", (n.as_str(),)).ok())
        .collect();
    names
        .into_iter()
        .zip(bus.call_all(msgs, TIMEOUT, None))
        .filter(|(_, r)| r.as_ref().and_then(|m| m.read1::<u32>().ok()).is_some_and(|p| pids.contains(&p)))
        .map(|(n, _)| n)
        .collect()
}

type Objects = HashMap<String, Vec<String>>;

/// Introspects every connection breadth-first, one pipelined batch per tree level.
/// `grace` cuts the first level short once one connection has answered.
fn object_trees(bus: &Bus, names: &[String], mut grace: Option<Duration>) -> Vec<(String, Objects)> {
    let mut trees: Vec<(String, Objects)> = names.iter().map(|n| (n.clone(), Objects::new())).collect();
    let mut level: Vec<(usize, String)> = (0..names.len()).map(|i| (i, "/".to_string())).collect();
    let mut cap = PROBE_CAP;
    while !level.is_empty() {
        let msgs = level.iter().filter_map(|(i, path)| bus::method_call(&names[*i], path, INTROSPECTABLE, "Introspect", ()).ok()).collect();
        let replies = bus.call_all(msgs, cap, grace.take());
        cap = TIMEOUT;
        let mut next = Vec::new();
        for ((i, path), reply) in level.into_iter().zip(replies) {
            let Some(xml) = reply.and_then(|m| m.read1::<String>().ok()) else { continue };
            let (interfaces, children) = parse_introspect(&xml);
            for child in children {
                next.push((i, format!("{}/{child}", path.trim_end_matches('/'))));
            }
            trees[i].1.insert(path, interfaces);
        }
        level = next;
    }
    trees
}

/// (interface names, child node names) from introspection XML.
fn parse_introspect(xml: &str) -> (Vec<String>, Vec<String>) {
    let attr = |tag: &str| -> Vec<String> {
        xml.split(tag).skip(1).filter_map(|s| s.split('"').next()).map(str::to_owned).collect()
    };
    let children = attr("<node name=\"").into_iter().filter(|n| !n.starts_with('/')).collect();
    (attr("<interface name=\""), children)
}

pub fn clean(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut chars = label.chars();
    while let Some(c) = chars.next() {
        if c == '_' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// 'toggle-sidebar' -> 'Toggle sidebar' for actions exported without labels.
pub fn humanize(action: &str) -> String {
    let words = action.replace(['-', '_'], " ").to_lowercase();
    let mut chars = words.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

fn modifier(m: &str) -> &str {
    match m {
        "Primary" | "Control" => "Ctrl",
        m => m,
    }
}

/// '<Primary><Shift>s' -> 'Ctrl+Shift+S'.
pub fn gtk_accel(accel: &str) -> String {
    let mut parts = Vec::new();
    let mut rest = accel;
    while let Some(after) = rest.strip_prefix('<') {
        let Some((m, tail)) = after.split_once('>') else { break };
        parts.push(modifier(m).to_string());
        rest = tail;
    }
    if !rest.is_empty() {
        parts.push(if rest.chars().count() == 1 { rest.to_uppercase() } else { rest.to_string() });
    }
    parts.join("+")
}

pub struct MenuItem {
    pub label: String,
    pub accel: String,
    pub action: Option<String>,
    pub target: Option<Box<dyn RefArg>>,
    pub section: Option<(u32, u32)>,
    pub submenu: Option<(u32, u32)>,
}

impl MenuItem {
    fn from_props(props: &PropMap) -> MenuItem {
        let s = |k: &str| props.get(k).and_then(|v| v.0.as_str()).map(str::to_owned);
        let link = |k: &str| {
            let mut ids = props.get(k)?.0.as_iter()?.filter_map(|a| a.as_u64());
            Some((ids.next()? as u32, ids.next()? as u32))
        };
        MenuItem {
            label: clean(&s("label").unwrap_or_default()),
            accel: s("accel").unwrap_or_default(),
            action: s("action"),
            target: props.get("target").map(|v| v.0.box_clone()),
            section: link(":section"),
            submenu: link(":submenu"),
        }
    }
}

pub type GtkMenus = HashMap<(u32, u32), Vec<MenuItem>>;

/// Flattens org.gtk.Menus groups into (label path, item) for items that carry an action.
pub fn walk_gtk_menu<'a>(menus: &'a GtkMenus, key: (u32, u32), path: &mut Vec<String>, out: &mut Vec<(String, &'a MenuItem)>) {
    if path.len() > MAX_DEPTH {
        return;
    }
    for item in menus.get(&key).into_iter().flatten() {
        if let Some(section) = item.section {
            walk_gtk_menu(menus, section, path, out);
        } else if let Some(submenu) = item.submenu {
            path.push(item.label.clone());
            walk_gtk_menu(menus, submenu, path, out);
            path.pop();
        } else if item.action.is_some() {
            let mut full = path.clone();
            full.push(item.label.clone());
            out.push((full.join(SEP), item));
        }
    }
}

/// (enabled, takes a parameter) for each action of an org.gtk.Actions object.
type Described = HashMap<String, (bool, bool)>;

fn describe_all(bus: &Bus, name: &str, path: &str) -> Described {
    type Raw = HashMap<String, (bool, Signature<'static>, Vec<Variant<Box<dyn RefArg>>>)>;
    bus.call(name, path, GTK_ACTIONS, "DescribeAll", ())
        .ok()
        .and_then(|m| m.read1::<Raw>().ok())
        .map(|raw| raw.into_iter().map(|(k, (enabled, sig, _))| (k, (enabled, !sig.is_empty()))).collect())
        .unwrap_or_default()
}

/// Commands from GtkApplication: menubar labels when exported, else bare action names.
fn gtk_commands(bus: &Bus, name: &str, objects: &Objects) -> Vec<Command> {
    let with = |iface: &'static str| objects.iter().filter(move |(_, ifs)| ifs.iter().any(|i| i == iface)).map(|(p, _)| p.clone());
    let window_number = |p: &String| p.rsplit('/').next().and_then(|n| n.parse::<u32>().ok()).unwrap_or(0);
    let mut windows: Vec<String> = with(GTK_ACTIONS).filter(|p| p.contains("/window/")).collect();
    windows.sort_by_key(window_number);
    let app = with(GTK_ACTIONS).find(|p| !p.contains("/window/"));
    // shortcut: the first window stands for all of them, the focused one may be another (BRIEF pitfall 4)
    let actions: HashMap<&str, (String, Described)> = [("app", app), ("win", windows.into_iter().next())]
        .into_iter()
        .filter_map(|(prefix, path)| path.map(|p| (prefix, (p.clone(), describe_all(bus, name, &p)))))
        .collect();

    let gtk = |prefix: &str, action: &str, target: Option<Box<dyn RefArg>>| Action::Gtk {
        name: name.to_string(),
        path: actions[prefix].0.clone(),
        action: action.to_string(),
        target,
    };

    let mut commands = Vec::new();
    for path in with(GTK_MENUS) {
        let menus = subscribe(bus, name, &path);
        let mut items = Vec::new();
        walk_gtk_menu(&menus, (0, 0), &mut Vec::new(), &mut items);
        for (label, item) in items {
            let Some((prefix, action)) = item.action.as_deref().and_then(|a| a.split_once('.')) else { continue };
            let Some(&(enabled, takes_param)) = actions.get(prefix).and_then(|(_, d)| d.get(action)) else { continue };
            if enabled && (item.target.is_some() || !takes_param) {
                let target = item.target.as_ref().map(|t| t.box_clone());
                commands.push(Command { label, accel: gtk_accel(&item.accel), action: gtk(prefix, action, target) });
            }
        }
    }
    if commands.is_empty() {
        for (prefix, (_, described)) in ["app", "win"].iter().filter_map(|p| actions.get_key_value(p)) {
            let mut names: Vec<_> = described.iter().filter(|(_, (enabled, takes_param))| *enabled && !takes_param).map(|(a, _)| a).collect();
            names.sort();
            for action in names {
                commands.push(Command { label: humanize(action), accel: String::new(), action: gtk(prefix, action, None) });
            }
        }
    }
    commands
}

/// Subscribes to group 0 and every group it references, then unsubscribes.
fn subscribe(bus: &Bus, name: &str, path: &str) -> GtkMenus {
    let mut menus = GtkMenus::new();
    let (mut pending, mut seen) = (vec![0u32], HashSet::new());
    while !pending.is_empty() {
        seen.extend(pending.iter().copied());
        let Ok(reply) = bus.call(name, path, GTK_MENUS, "Start", (pending.clone(),)) else { break };
        let groups: Vec<(u32, u32, Vec<PropMap>)> = reply.read1().unwrap_or_default();
        pending.clear();
        for (group, menu, items) in groups {
            let items: Vec<MenuItem> = items.iter().map(MenuItem::from_props).collect();
            for (g, _) in items.iter().filter_map(|i| i.section.or(i.submenu)) {
                if !seen.contains(&g) && !pending.contains(&g) {
                    pending.push(g);
                }
            }
            menus.insert((group, menu), items);
        }
    }
    bus.send(name, path, GTK_MENUS, "End", (seen.into_iter().collect::<Vec<u32>>(),));
    menus
}

#[derive(Debug, PartialEq)]
pub struct MenuNode {
    pub id: i32,
    pub label: String,
    pub visible: bool,
    pub enabled: bool,
    pub separator: bool,
    pub shortcut: Vec<String>,
    pub children: Vec<MenuNode>,
}

fn unvariant(a: &dyn RefArg) -> &dyn RefArg {
    match a.arg_type() {
        ArgType::Variant => a.as_iter().and_then(|mut i| i.next()).unwrap_or(a),
        _ => a,
    }
}

impl MenuNode {
    /// Parses a dbusmenu (ia{sv}av) layout node.
    fn parse(node: &dyn RefArg) -> Option<MenuNode> {
        let mut fields = unvariant(node).as_iter()?;
        let id = fields.next()?.as_i64()? as i32;
        let mut parsed = MenuNode { id, label: String::new(), visible: true, enabled: true, separator: false, shortcut: vec![], children: vec![] };
        let mut props = fields.next()?.as_iter()?;
        while let (Some(key), Some(value)) = (props.next(), props.next()) {
            let value = unvariant(value);
            match key.as_str()? {
                "label" => parsed.label = clean(value.as_str().unwrap_or_default()),
                "visible" => parsed.visible = value.as_i64() != Some(0),
                "enabled" => parsed.enabled = value.as_i64() != Some(0),
                "type" => parsed.separator = value.as_str() == Some("separator"),
                "shortcut" => {
                    let first = value.as_iter().and_then(|mut i| i.next());
                    parsed.shortcut = first.and_then(|k| k.as_iter()).into_iter().flatten().filter_map(|k| k.as_str()).map(|k| modifier(k).to_string()).collect();
                }
                _ => {}
            }
        }
        parsed.children = fields.next()?.as_iter()?.filter_map(MenuNode::parse).collect();
        Some(parsed)
    }

    fn submenu_ids(&self, out: &mut Vec<i32>) {
        for child in self.children.iter().filter(|c| !c.children.is_empty()) {
            out.push(child.id);
            child.submenu_ids(out);
        }
    }
}

/// Flattens a dbusmenu layout into (label path, shortcut, id), skipping hidden and disabled items.
pub fn walk_dbusmenu(node: &MenuNode, path: &mut Vec<String>, out: &mut Vec<(String, String, i32)>) {
    if path.len() > MAX_DEPTH {
        return;
    }
    for child in node.children.iter().filter(|c| c.visible && c.enabled && !c.separator) {
        path.push(child.label.clone());
        if !child.children.is_empty() {
            walk_dbusmenu(child, path, out);
        } else if !child.label.is_empty() {
            out.push((path.join(SEP), child.shortcut.join("+"), child.id));
        }
        path.pop();
    }
}

fn dbusmenu_commands(bus: &Bus, name: &str, path: &str) -> Vec<Command> {
    let layout = || -> Option<MenuNode> {
        let reply = bus.call(name, path, DBUSMENU, "GetLayout", (0i32, -1i32, Vec::<String>::new())).ok()?;
        let mut args = reply.iter_init();
        args.next();
        MenuNode::parse(&args.get_refarg()?)
    };
    let Some(mut root) = layout() else { return vec![] };
    // Dynamic submenus (recent files etc.) fill themselves in on AboutToShow.
    let mut ids = Vec::new();
    root.submenu_ids(&mut ids);
    if !ids.is_empty() && bus.call(name, path, DBUSMENU, "AboutToShowGroup", (ids,)).is_ok() {
        root = layout().unwrap_or(root);
    }
    let mut items = Vec::new();
    walk_dbusmenu(&root, &mut Vec::new(), &mut items);
    items
        .into_iter()
        .map(|(label, accel, id)| Command { label, accel, action: Action::DbusMenu { name: name.to_string(), path: path.to_string(), id } })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(clean("_File"), "File");
        assert_eq!(clean("Pasek _stanu"), "Pasek stanu");
        assert_eq!(clean("snake__case"), "snake_case");
        assert_eq!(humanize("toggle-sidebar"), "Toggle sidebar");
        assert_eq!(gtk_accel("<Primary><Shift>s"), "Ctrl+Shift+S");
        assert_eq!(gtk_accel("F11"), "F11");
        assert_eq!(gtk_accel(""), "");
    }

    #[test]
    fn introspect_xml() {
        let xml = r#"<node name="/org/app"><interface name="org.gtk.Actions"><method name="List"/></interface><node name="window"/><node name="menus"/></node>"#;
        assert_eq!(parse_introspect(xml), (vec!["org.gtk.Actions".into()], vec!["window".into(), "menus".into()]));
    }

    fn item(label: &str, action: Option<&str>, section: Option<(u32, u32)>, submenu: Option<(u32, u32)>) -> MenuItem {
        MenuItem { label: label.into(), accel: String::new(), action: action.map(Into::into), target: None, section, submenu }
    }

    #[test]
    fn gtk_menu_sections_and_submenus() {
        let menus = GtkMenus::from([
            ((0, 0), vec![item("File", None, None, Some((1, 0)))]),
            ((1, 0), vec![item("", None, Some((1, 1)), None), item("Quit", Some("app.quit"), None, None)]),
            ((1, 1), vec![item("Layout", Some("win.layout"), None, None)]),
        ]);
        let mut out = Vec::new();
        walk_gtk_menu(&menus, (0, 0), &mut Vec::new(), &mut out);
        let labels: Vec<_> = out.iter().map(|(l, i)| (l.as_str(), i.action.as_deref().unwrap())).collect();
        assert_eq!(labels, [("File › Layout", "win.layout"), ("File › Quit", "app.quit")]);
    }

    fn node(id: i32, label: &str, children: Vec<MenuNode>) -> MenuNode {
        MenuNode { id, label: label.into(), visible: true, enabled: true, separator: false, shortcut: vec![], children }
    }

    #[test]
    fn dbusmenu_skips_hidden_disabled_and_separators() {
        let mut save = node(2, "Save", vec![]);
        save.shortcut = vec!["Ctrl".into(), "S".into()];
        let mut sep = node(3, "", vec![]);
        sep.separator = true;
        let mut hidden = node(4, "Hidden", vec![]);
        hidden.visible = false;
        let mut off = node(5, "Off", vec![]);
        off.enabled = false;
        let root = node(0, "", vec![node(1, "File", vec![save, sep, hidden, off])]);
        let mut out = Vec::new();
        walk_dbusmenu(&root, &mut Vec::new(), &mut out);
        assert_eq!(out, [("File › Save".to_string(), "Ctrl+S".to_string(), 2)]);
    }

    #[test]
    fn dbusmenu_parses_wire_layout() {
        let props = |label: &str| -> PropMap { PropMap::from([("label".to_string(), Variant(Box::new(label.to_string()) as Box<dyn RefArg>))]) };
        let child: (i32, PropMap, Vec<Variant<Box<dyn RefArg>>>) = (7, props("_Save"), vec![]);
        let root: (i32, PropMap, Vec<Variant<Box<dyn RefArg>>>) = (0, props(""), vec![Variant(Box::new(child))]);
        let parsed = MenuNode::parse(&root).unwrap();
        assert_eq!(parsed.children, [node(7, "Save", vec![])]);
    }
}
