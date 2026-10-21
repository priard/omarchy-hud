//! Recently picked commands per app, shown first like Unity's HUD did.
//! Stored as "app<TAB>label" lines, most recent first, in $XDG_STATE_HOME/omarchy-hud/recent.

use std::path::PathBuf;
use std::{env, fs};

const PER_APP: usize = 10;

fn path() -> Option<PathBuf> {
    let state = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(state.join("omarchy-hud/recent"))
}

/// The app's process name, stable across runs unlike its pid.
pub fn app_key(pid: u32) -> String {
    fs::read_to_string(format!("/proc/{pid}/comm")).map(|c| c.trim().to_string()).unwrap_or_default()
}

fn load() -> Vec<(String, String)> {
    let Some(text) = path().and_then(|p| fs::read_to_string(p).ok()) else { return vec![] };
    text.lines().filter_map(|l| l.split_once('\t')).map(|(a, l)| (a.to_string(), l.to_string())).collect()
}

/// Labels this app picked recently, most recent first.
pub fn labels(app: &str) -> Vec<String> {
    load().into_iter().filter(|(a, _)| a == app).map(|(_, l)| l).collect()
}

/// Moves `label` to the front of `app`'s history and keeps PER_APP entries per app.
pub fn record(entries: Vec<(String, String)>, app: &str, label: &str) -> Vec<(String, String)> {
    let mut out = vec![(app.to_string(), label.to_string())];
    let mut kept = 0;
    for (a, l) in entries {
        if a == app {
            if l == label || kept + 1 >= PER_APP {
                continue;
            }
            kept += 1;
        }
        out.push((a, l));
    }
    out
}

pub fn remember(app: &str, label: &str) {
    let (Some(path), false) = (path(), app.is_empty() || label.contains('\n')) else { return };
    let text: String = record(load(), app, label).into_iter().map(|(a, l)| format!("{a}\t{l}\n")).collect();
    // Write then rename, so a crash never leaves a half-written history.
    let tmp = path.with_extension("tmp");
    let _ = path.parent().map(fs::create_dir_all);
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(&tmp, &path);
    }
}

/// Stable reorder: recent labels first in recency order, the rest in menu order.
pub fn promote<T>(items: &mut Vec<T>, label: impl Fn(&T) -> &str, recent: &[String]) {
    let rank = |t: &T| recent.iter().position(|r| r == label(t)).unwrap_or(usize::MAX);
    items.sort_by_key(|t| rank(t));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(a: &str, l: &str) -> (String, String) {
        (a.into(), l.into())
    }

    #[test]
    fn record_moves_to_front_and_caps_per_app() {
        let entries = vec![e("x", "Save"), e("y", "Open"), e("x", "Quit")];
        assert_eq!(record(entries, "x", "Quit"), [e("x", "Quit"), e("x", "Save"), e("y", "Open")]);

        let many: Vec<_> = (0..20).map(|i| e("x", &i.to_string())).chain([e("y", "keep")]).collect();
        let out = record(many, "x", "new");
        assert_eq!(out.iter().filter(|(a, _)| a == "x").count(), PER_APP);
        assert_eq!(out[0], e("x", "new"));
        assert!(out.contains(&e("y", "keep")));
    }

    #[test]
    fn promote_is_stable() {
        let mut items = vec!["a", "b", "c", "d"];
        promote(&mut items, |s| s, &["c".to_string(), "a".to_string(), "gone".to_string()]);
        assert_eq!(items, ["c", "a", "b", "d"]);
    }
}
