//! Labels for GTK actions that are exported without any. GTK4 apps keep their menus
//! in GtkBuilder files compiled into the binary (an ELF `.gresource.*` section, a GVDB
//! file), where each item pairs a label with an action. Labels are translated with the
//! app's gettext catalog, so `win.close-current-view` becomes "Zamknij kartę".

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::raw::{c_int, c_ulong};
use std::{env, fs};

use crate::menus::clean;

#[link(name = "z")]
unsafe extern "C" {
    fn uncompress(
        dest: *mut u8,
        dest_len: *mut c_ulong,
        source: *const u8,
        source_len: c_ulong,
    ) -> c_int;
}

#[derive(Debug, PartialEq)]
pub struct UiItem {
    /// "win.close-current-view"
    pub action: String,
    /// The target as written in the .ui file, e.g. "fit-page" or "'fit-page'".
    pub target: Option<String>,
    /// Translated, mnemonic-free label, prefixed with its submenus.
    pub label: String,
}

/// Menu items from the GtkBuilder files compiled into the process's executable.
pub fn for_pid(pid: u32) -> Vec<UiItem> {
    let Ok(exe) = fs::read_link(format!("/proc/{pid}/exe")) else {
        return vec![];
    };
    let Some(resources) = gresource_section(&exe.to_string_lossy()) else {
        return vec![];
    };
    let default_domain = exe
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut catalogs: HashMap<String, Catalog> = HashMap::new();
    let mut items = Vec::new();
    for xml in gvdb_files(&resources, ".ui") {
        let Ok(xml) = String::from_utf8(xml) else {
            continue;
        };
        let domain = attr(
            &xml[..xml
                .find("<interface")
                .map_or(0, |i| (i + 200).min(xml.len()))],
            "domain",
        )
        .unwrap_or_else(|| default_domain.clone());
        let catalog = catalogs
            .entry(domain.clone())
            .or_insert_with(|| Catalog::load(&domain));
        items.extend(parse_ui(&xml, &|text, context| catalog.get(text, context)));
    }
    items
}

fn read_at(file: &mut File, offset: u64, len: usize) -> Option<Vec<u8>> {
    let mut buf = vec![0; len];
    file.seek(SeekFrom::Start(offset)).ok()?;
    file.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], i: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(i..i + 8)?.try_into().ok()?))
}

/// The first `.gresource.*` section of a 64-bit little-endian ELF file.
fn gresource_section(path: &str) -> Option<Vec<u8>> {
    let mut file = File::open(path).ok()?;
    let header = read_at(&mut file, 0, 64)?;
    if &header[..4] != b"\x7fELF" || header[4] != 2 || header[5] != 1 {
        return None;
    }
    let (shoff, shentsize, shnum, shstrndx) = (
        u64_at(&header, 0x28)?,
        u16_at(&header, 0x3a)? as usize,
        u16_at(&header, 0x3c)? as usize,
        u16_at(&header, 0x3e)? as usize,
    );
    let sections = read_at(&mut file, shoff, shentsize * shnum)?;
    let section = |i: usize| -> Option<(u32, u64, u64)> {
        let s = sections.get(i * shentsize..(i + 1) * shentsize)?;
        Some((u32_at(s, 0)?, u64_at(s, 0x18)?, u64_at(s, 0x20)?))
    };
    let (_, names_off, names_size) = section(shstrndx)?;
    let names = read_at(&mut file, names_off, names_size as usize)?;
    (0..shnum).find_map(|i| {
        let (name, offset, size) = section(i)?;
        let name = names.get(name as usize..)?.split(|&b| b == 0).next()?;
        name.starts_with(b".gresource.")
            .then(|| read_at(&mut file, offset, size as usize))
            .flatten()
    })
}

/// Contents of the files in a GResource (GVDB) blob whose names end with `suffix`.
fn gvdb_files(data: &[u8], suffix: &str) -> Vec<Vec<u8>> {
    let Some((start, end)) = (|| {
        (data.get(..8)? == b"GVariant").then_some(())?;
        Some((u32_at(data, 16)? as usize, u32_at(data, 20)? as usize))
    })() else {
        return vec![];
    };
    let Some(root) = data.get(start..end) else {
        return vec![];
    };
    let (Some(bloom), Some(buckets)) = (u32_at(root, 0), u32_at(root, 4)) else {
        return vec![];
    };
    let items_start = 8 + 4 * ((bloom & ((1 << 27) - 1)) as usize + buckets as usize);
    let items: Vec<&[u8]> = root
        .get(items_start..)
        .unwrap_or_default()
        .chunks_exact(24)
        .collect();
    let key = |item: &[u8]| -> Option<&[u8]> {
        let (start, size) = (u32_at(item, 8)? as usize, u16_at(item, 12)? as usize);
        data.get(start..start + size)
    };
    let full_name = |mut i: usize| -> Option<Vec<u8>> {
        let mut parts = Vec::new();
        for _ in 0..64 {
            let item = items.get(i)?;
            parts.push(key(item)?);
            match u32_at(item, 4)? {
                u32::MAX => break,
                parent => i = parent as usize,
            }
        }
        Some(parts.into_iter().rev().flatten().copied().collect())
    };
    let mut files = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if item[14] != b'v' || !full_name(i).is_some_and(|n| n.ends_with(suffix.as_bytes())) {
            continue;
        }
        let (Some(vstart), Some(vend)) = (u32_at(item, 16), u32_at(item, 20)) else {
            continue;
        };
        // A 'v' value is the serialized (uuay) tuple, a NUL, then its type string.
        let Some(value) = data.get(vstart as usize..vend as usize) else {
            continue;
        };
        let Some(nul) = value.iter().rposition(|&b| b == 0) else {
            continue;
        };
        let (Some(size), Some(flags), Some(body)) =
            (u32_at(value, 0), u32_at(value, 4), value.get(8..nul))
        else {
            continue;
        };
        if flags & 1 == 0 {
            files.push(body[..(size as usize).min(body.len())].to_vec());
        } else if let Some(inflated) = inflate(body, size as usize) {
            files.push(inflated);
        }
    }
    files
}

fn inflate(data: &[u8], size: usize) -> Option<Vec<u8>> {
    let mut out = vec![0u8; size];
    let mut len = size as c_ulong;
    // SAFETY: both buffers are valid for the lengths passed; zlib writes at most `len` bytes.
    let status = unsafe {
        uncompress(
            out.as_mut_ptr(),
            &mut len,
            data.as_ptr(),
            data.len() as c_ulong,
        )
    };
    (status == 0).then(|| {
        out.truncate(len as usize);
        out
    })
}

/// A gettext .mo catalog for the user's language, or an empty one.
pub struct Catalog {
    translations: HashMap<Vec<u8>, String>,
}

impl Catalog {
    fn load(domain: &str) -> Catalog {
        let data = languages()
            .iter()
            .find_map(|lang| {
                fs::read(format!("/usr/share/locale/{lang}/LC_MESSAGES/{domain}.mo")).ok()
            })
            .unwrap_or_default();
        Catalog::parse(data)
    }

    /// Parses a little-endian .mo file; plural forms keep only their singular.
    pub fn parse(data: Vec<u8>) -> Catalog {
        let mut translations = HashMap::new();
        if u32_at(&data, 0) == Some(0x950412de) {
            let field = |i: usize| u32_at(&data, i).unwrap_or(0) as usize;
            let (n, originals, translated) = (field(8), field(12), field(16));
            let string = |table: usize, i: usize| -> Option<&[u8]> {
                let (len, offset) = (
                    u32_at(&data, table + 8 * i)? as usize,
                    u32_at(&data, table + 8 * i + 4)? as usize,
                );
                data.get(offset..offset + len)?.split(|&b| b == 0).next()
            };
            for i in 0..n {
                if let (Some(o), Some(t)) = (
                    string(originals, i),
                    string(translated, i).and_then(|t| std::str::from_utf8(t).ok()),
                ) {
                    if !t.is_empty() {
                        translations.insert(o.to_vec(), t.to_string());
                    }
                }
            }
        }
        Catalog { translations }
    }

    pub fn get(&self, text: &str, context: Option<&str>) -> String {
        let key = match context {
            Some(c) => format!("{c}\x04{text}"),
            None => text.to_string(),
        };
        self.translations
            .get(key.as_bytes())
            .cloned()
            .unwrap_or_else(|| text.to_string())
    }
}

/// gettext's language list: LANGUAGE, then LC_ALL, LC_MESSAGES, LANG, each also without region.
fn languages() -> Vec<String> {
    let mut langs = Vec::new();
    let mut add = |l: &str| {
        let l = l.split(['.', '@']).next().unwrap_or_default();
        if !l.is_empty() && l != "C" && l != "POSIX" {
            for candidate in [l, l.split('_').next().unwrap_or(l)] {
                if !langs.iter().any(|x| x == candidate) {
                    langs.push(candidate.to_string());
                }
            }
        }
    };
    if let Ok(list) = env::var("LANGUAGE") {
        list.split(':').for_each(&mut add);
    }
    if let Some(l) = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|v| env::var(v).ok().filter(|l| !l.is_empty()))
    {
        add(&l);
    }
    langs
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
    Some(unescape(&tag[start..start + tag[start..].find('"')?]))
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let Some(end) = rest[i..].find(';') else {
            out.push_str(&rest[i..]);
            return out;
        };
        let entity = &rest[i + 1..i + end];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            e => match e
                .strip_prefix("#x")
                .map(|h| u32::from_str_radix(h, 16))
                .or_else(|| e.strip_prefix('#').map(|d| d.parse()))
            {
                Some(Ok(c)) => out.extend(char::from_u32(c)),
                _ => out.push_str(&rest[i..i + end + 1]),
            },
        }
        rest = &rest[i + end + 1..];
    }
    out.push_str(rest);
    out
}

enum Frame {
    Submenu(Option<String>),
    Item(HashMap<String, String>),
    Object(HashMap<String, String>),
    Other,
}

/// Menu items (`<item>` with action and label) and widgets (`action-name` with label
/// or tooltip) of a GtkBuilder file. `translate(text, context)` localizes labels.
pub fn parse_ui(xml: &str, translate: &dyn Fn(&str, Option<&str>) -> String) -> Vec<UiItem> {
    let mut items = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut rest = xml;
    while let Some(open) = rest.find('<') {
        rest = &rest[open..];
        if rest.starts_with("<!--") {
            rest = rest.find("-->").map_or("", |e| &rest[e + 3..]);
            continue;
        }
        let Some(close) = rest.find('>') else { break };
        let tag = &rest[..close + 1];
        rest = &rest[close + 1..];
        let self_closing = tag.ends_with("/>");
        let closing = tag[1..].starts_with('/');
        let name = tag[1..]
            .trim_start_matches('/')
            .split([' ', '>', '/', '\n', '\t'])
            .next()
            .unwrap_or_default();

        match (closing, name) {
            (
                true,
                "submenu" | "item" | "object" | "menu" | "section" | "child" | "interface"
                | "template",
            ) => {
                let path: Vec<String> = stack
                    .iter()
                    .filter_map(|f| {
                        if let Frame::Submenu(Some(l)) = f {
                            Some(clean(l))
                        } else {
                            None
                        }
                    })
                    .collect();
                let labelled = |label: &str| {
                    path.iter()
                        .cloned()
                        .chain([clean(label)])
                        .collect::<Vec<_>>()
                        .join(" › ")
                };
                match stack.pop() {
                    Some(Frame::Item(a)) => {
                        if let (Some(action), Some(label)) = (a.get("action"), a.get("label")) {
                            items.push(UiItem {
                                action: action.clone(),
                                target: a.get("target").cloned(),
                                label: labelled(label),
                            });
                        }
                    }
                    Some(Frame::Object(p)) => {
                        if let (Some(action), Some(label)) = (
                            p.get("action-name"),
                            p.get("label").or(p.get("tooltip-text")),
                        ) {
                            items.push(UiItem {
                                action: action.clone(),
                                target: p.get("action-target").cloned(),
                                label: clean(label),
                            });
                        }
                    }
                    _ => {}
                }
            }
            (true, _) => {}
            (false, name) => match name {
                "submenu" if !self_closing => stack.push(Frame::Submenu(None)),
                "item" if !self_closing => stack.push(Frame::Item(HashMap::new())),
                "object" if !self_closing => stack.push(Frame::Object(HashMap::new())),
                "menu" | "section" | "child" | "interface" | "template" if !self_closing => {
                    stack.push(Frame::Other)
                }
                "attribute" | "property" if !self_closing => {
                    let Some(end) = rest.find('<') else { break };
                    let text = unescape(rest[..end].trim());
                    rest = &rest[end..];
                    let key = attr(tag, "name").unwrap_or_default();
                    let text = if attr(tag, "translatable").as_deref() == Some("yes") {
                        translate(&text, attr(tag, "context").as_deref())
                    } else {
                        text
                    };
                    match stack.last_mut() {
                        Some(Frame::Item(attrs) | Frame::Object(attrs)) => {
                            attrs.insert(key, text);
                        }
                        Some(Frame::Submenu(label)) if key == "label" => *label = Some(text),
                        _ => {}
                    }
                }
                _ => {}
            },
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_menus_and_widgets() {
        let xml = r#"<?xml version="1.0"?>
<interface>
  <!-- <item><attribute name="action">win.ignored</attribute></item> -->
  <menu id="m">
    <section>
      <item>
        <attribute name="label" translatable="yes">_New Window</attribute>
        <attribute name="action">app.new-window</attribute>
      </item>
    </section>
    <submenu>
      <attribute name="label" translatable="yes">_Zoom</attribute>
      <item>
        <attribute name="label" translatable="yes" context="zoom">Fit &amp; Page</attribute>
        <attribute name="action">win.sizing-mode</attribute>
        <attribute name="target">fit-page</attribute>
      </item>
    </submenu>
  </menu>
  <object class="GtkButton">
    <property name="tooltip-text" translatable="yes">Show Sidebar</property>
    <property name="action-name">win.show-sidebar</property>
    <child><object class="GtkImage"><property name="icon-name">x</property></object></child>
  </object>
</interface>"#;
        let tr = |t: &str, c: Option<&str>| {
            if c == Some("zoom") {
                format!("[{t}]")
            } else {
                t.to_string()
            }
        };
        assert_eq!(
            parse_ui(xml, &tr),
            [
                UiItem {
                    action: "app.new-window".into(),
                    target: None,
                    label: "New Window".into()
                },
                UiItem {
                    action: "win.sizing-mode".into(),
                    target: Some("fit-page".into()),
                    label: "Zoom › [Fit & Page]".into()
                },
                UiItem {
                    action: "win.show-sidebar".into(),
                    target: None,
                    label: "Show Sidebar".into()
                },
            ]
        );
    }

    fn mo(pairs: &[(&str, &str)]) -> Vec<u8> {
        let n = pairs.len() as u32;
        let (orig_table, trans_table) = (28u32, 28 + 8 * n);
        let mut strings = Vec::new();
        let mut header = vec![0x950412deu32, 0, n, orig_table, trans_table, 0, 0];
        let base = 28 + 16 * n;
        let mut origs = vec![];
        let mut trans = vec![];
        for (o, _) in pairs {
            origs.extend([o.len() as u32, base + strings.len() as u32]);
            strings.extend(o.as_bytes());
            strings.push(0);
        }
        for (_, t) in pairs {
            trans.extend([t.len() as u32, base + strings.len() as u32]);
            strings.extend(t.as_bytes());
            strings.push(0);
        }
        header.extend(origs);
        header.extend(trans);
        header
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .chain(strings)
            .collect()
    }

    #[test]
    fn catalog_translates_with_context() {
        let catalog = Catalog::parse(mo(&[
            ("_New Window", "_Nowe okno"),
            ("zoom\x04Fit", "Dopasuj"),
        ]));
        assert_eq!(catalog.get("_New Window", None), "_Nowe okno");
        assert_eq!(catalog.get("Fit", Some("zoom")), "Dopasuj");
        assert_eq!(catalog.get("Missing", None), "Missing");
        assert_eq!(Catalog::parse(vec![1, 2, 3]).get("x", None), "x");
    }

    #[test]
    fn unescapes_entities() {
        assert_eq!(unescape("a &amp; b &#x41;&#66; &bogus"), "a & b AB &bogus");
    }
}
