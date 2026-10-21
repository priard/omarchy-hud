//! Hyprland's request socket: the focused window and which of its app's windows it is.
//! Queried directly, which is faster than spawning hyprctl.

use std::env;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

pub struct Window {
    pub address: String,
    pub pid: u32,
    pub floating: bool,
    pub stable_id: u64,
}

/// Where the focused window sits among its app's tiled windows, in creation order.
/// GTK numbers its exported windows in the same order, so this picks the matching one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Focus {
    pub index: usize,
    pub count: usize,
}

fn request(command: &str) -> Result<String, String> {
    let dir = env::var("XDG_RUNTIME_DIR").map_err(|_| "XDG_RUNTIME_DIR is not set")?;
    let sig = env::var("HYPRLAND_INSTANCE_SIGNATURE").map_err(|_| "not running under Hyprland")?;
    let mut sock = UnixStream::connect(format!("{dir}/hypr/{sig}/.socket.sock"))
        .map_err(|e| format!("hyprland socket: {e}"))?;
    sock.write_all(command.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut reply = String::new();
    sock.read_to_string(&mut reply).map_err(|e| e.to_string())?;
    Ok(reply)
}

/// Parses `clients` (and `activewindow`) text output.
pub fn parse_windows(text: &str) -> Vec<Window> {
    text.split("Window ")
        .skip(1)
        .filter_map(|block| {
            let address = block
                .split_whitespace()
                .next()?
                .trim_start_matches("0x")
                .to_string();
            let field = |name: &str| {
                block
                    .lines()
                    .find_map(|l| l.trim().strip_prefix(name)?.strip_prefix(": "))
            };
            Some(Window {
                address,
                pid: field("pid")?.parse().ok()?,
                floating: field("floating") == Some("1"),
                stable_id: u64::from_str_radix(field("stableID").unwrap_or("0"), 16).unwrap_or(0),
            })
        })
        .collect()
}

pub fn focus_of(windows: &[Window], address: &str) -> Option<Focus> {
    let focused = windows.iter().find(|w| w.address == address)?;
    if focused.floating {
        return None;
    }
    let mut tiled: Vec<&Window> = windows
        .iter()
        .filter(|w| w.pid == focused.pid && !w.floating)
        .collect();
    tiled.sort_by_key(|w| w.stable_id);
    Some(Focus {
        index: tiled.iter().position(|w| w.address == address)?,
        count: tiled.len(),
    })
}

/// The pid of the given window (an address, with or without 0x) or the focused one.
pub fn window(address: Option<&str>) -> Result<(u32, Option<Focus>), String> {
    let address = match address {
        Some(a) => a.trim_start_matches("0x").to_string(),
        None => {
            parse_windows(&request("activewindow")?)
                .into_iter()
                .next()
                .ok_or("no focused window")?
                .address
        }
    };
    let windows = parse_windows(&request("clients")?);
    let pid = windows
        .iter()
        .find(|w| w.address == address)
        .ok_or_else(|| format!("no window {address}"))?
        .pid;
    Ok((pid, focus_of(&windows, &address)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIENTS: &str = "Window 55e578846200 -> Bez tytułu 1 — LibreOffice Writer:
\tfloating: 0
\tpid: 42
\tstableID: 18000101

Window 55e578421600 -> Porada dnia: 2/224:
\tfloating: 1
\tpid: 42
\tstableID: 18000102

Window 55e5787d28f0 -> Bez tytułu 2 — LibreOffice Writer:
\tfloating: 0
\tpid: 42
\tstableID: 1800010a

Window 55e578605800 -> Mattermost:
\tfloating: 0
\tpid: 7
\tstableID: 18000005
";

    #[test]
    fn focus_counts_tiled_windows_of_the_app_in_creation_order() {
        let windows = parse_windows(CLIENTS);
        assert_eq!(windows.len(), 4);
        assert_eq!(
            focus_of(&windows, "55e5787d28f0"),
            Some(Focus { index: 1, count: 2 })
        );
        assert_eq!(
            focus_of(&windows, "55e578846200"),
            Some(Focus { index: 0, count: 2 })
        );
        assert_eq!(focus_of(&windows, "55e578421600"), None);
        assert_eq!(
            focus_of(&windows, "55e578605800"),
            Some(Focus { index: 0, count: 1 })
        );
    }
}
