//! Shortcut packs: commands for apps that export no menu, run by sending their key chord
//! to the window. Packs are `[class class...]` sections of `Label = Ctrl+Shift+T` lines;
//! the built-in ones live in packs/. Earlier sources win for the same label: the user's
//! $XDG_CONFIG_HOME/omarchy-hud/shortcuts/, then packs apps install in
//! $XDG_DATA_HOME and $XDG_DATA_DIRS (omarchy-hud/shortcuts/), then the built-in ones.

use std::path::PathBuf;
use std::{env, fs};

const BUILTIN: &[&str] = &[
    include_str!("../packs/chromium.conf"),
    include_str!("../packs/firefox.conf"),
    include_str!("../packs/flea.conf"),
    include_str!("../packs/rawmakase.conf"),
    include_str!("../packs/thunderbird.conf"),
];

/// (label, accel) lines of the sections listing `class`, in file order.
pub fn parse(text: &str, class: &str) -> Vec<(String, String)> {
    let mut active = false;
    let mut out = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(classes) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            active = classes
                .split_whitespace()
                .any(|c| c.eq_ignore_ascii_case(class));
        } else if let (true, Some((label, accel))) = (active, line.rsplit_once(" = ")) {
            if chord(accel).is_some() {
                out.push((label.trim().to_string(), accel.trim().to_string()));
            }
        }
    }
    out
}

/// Pack directories, most important first.
fn pack_dirs() -> Vec<PathBuf> {
    let home = env::var_os("HOME").map(PathBuf::from);
    let var = |name: &str, fallback: &str| {
        env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(fallback)))
    };
    let data_dirs = env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    [var("XDG_CONFIG_HOME", ".config"), var("XDG_DATA_HOME", ".local/share")]
        .into_iter()
        .flatten()
        .chain(data_dirs.split(':').map(PathBuf::from))
        .map(|d| d.join("omarchy-hud/shortcuts"))
        .collect()
}

fn installed_packs() -> Vec<String> {
    let mut packs = Vec::new();
    for dir in pack_dirs() {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "conf"))
            .collect();
        paths.sort();
        packs.extend(paths.iter().filter_map(|p| fs::read_to_string(p).ok()));
    }
    packs
}

/// The window class's commands, the most important source first.
pub fn for_class(class: &str) -> Vec<(String, String)> {
    if class.is_empty() {
        return vec![];
    }
    let installed = installed_packs();
    installed
        .iter()
        .map(String::as_str)
        .chain(BUILTIN.iter().copied())
        .flat_map(|text| parse(text, class))
        .collect()
}

/// 'Ctrl+Shift+T' -> ("CTRL SHIFT", "T"). None unless every part is a plain name,
/// since both end up inside a Hyprland Lua call.
pub fn chord(accel: &str) -> Option<(String, String)> {
    let mut parts: Vec<&str> = accel.split('+').map(str::trim).collect();
    let key = parts.pop()?;
    let plain = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !plain(key) {
        return None;
    }
    let mods = parts
        .iter()
        .map(|m| match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Some("CTRL"),
            "shift" => Some("SHIFT"),
            "alt" => Some("ALT"),
            "super" => Some("SUPER"),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some((mods.join(" "), key.to_string()))
}

/// 'Ctrl+slash' -> 'Ctrl+/': xkb names of punctuation read as the key itself.
pub fn display(accel: &str) -> String {
    accel
        .split('+')
        .map(|part| match part {
            "slash" => "/",
            "backslash" => "\\",
            "comma" => ",",
            "period" => ".",
            "equal" => "=",
            "minus" => "-",
            "apostrophe" => "'",
            "grave" => "`",
            "bracketleft" => "[",
            "bracketright" => "]",
            "Page_Up" => "PgUp",
            "Page_Down" => "PgDn",
            "Escape" => "Esc",
            part => part,
        })
        .collect::<Vec<_>>()
        .join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACK: &str = "# browsers
[zen firefox]
New tab = Ctrl+T
Bad = Ctrl+\"
Bad too = Hyper+T

[chromium]
History = Ctrl+H
";

    #[test]
    fn parse_picks_the_class_sections() {
        assert_eq!(parse(PACK, "Zen"), [("New tab".into(), "Ctrl+T".into())]);
        assert_eq!(parse(PACK, "chromium"), [("History".into(), "Ctrl+H".into())]);
        assert!(parse(PACK, "foot").is_empty());
    }

    #[test]
    fn display_reads_punctuation_as_keys() {
        assert_eq!(display("Ctrl+slash"), "Ctrl+/");
        assert_eq!(display("Ctrl+Shift+bracketright"), "Ctrl+Shift+]");
        assert_eq!(display("Ctrl+Page_Down"), "Ctrl+PgDn");
        assert_eq!(display("Shift+F5"), "Shift+F5");
    }

    #[test]
    fn chord_maps_mods_and_rejects_odd_keys() {
        assert_eq!(chord("Ctrl+Shift+Delete"), Some(("CTRL SHIFT".into(), "Delete".into())));
        assert_eq!(chord("F11"), Some(("".into(), "F11".into())));
        assert_eq!(chord("Ctrl+\")"), None);
        assert_eq!(chord("Ctrl+"), None);
    }

    #[test]
    fn builtin_pack_lines_all_parse() {
        for line in BUILTIN.iter().flat_map(|t| t.lines()).map(str::trim) {
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            let accel = line.rsplit_once(" = ").map(|(_, a)| a);
            assert!(accel.and_then(chord).is_some(), "bad pack line: {line}");
        }
    }
}
