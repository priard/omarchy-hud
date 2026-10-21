//! omarchy-hud: search and run the menu commands of the focused window.

mod bus;
mod labels;
mod menus;
mod recent;
mod registrar;

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::Duration;
use std::{env, fs, thread};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const USAGE: &str = "usage: omarchy-hud [--pid PID] [--list | --run LABEL] [--version]
       omarchy-hud registrar

Search and run the menu commands of the focused window.

  --pid PID    use this process instead of the focused window
  --list       print the commands instead of showing the menu
  --run LABEL  run the command with this label without the menu
  registrar    run the AppMenu registrar Qt needs to export its menus";

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("omarchy-hud: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let (mut pid, mut list, mut label) = (None, false, None);
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pid" => {
                pid = Some(
                    args.next()
                        .and_then(|p| p.parse().ok())
                        .ok_or("--pid needs a number")?,
                )
            }
            "--list" => list = true,
            "--run" => label = Some(args.next().ok_or("--run needs a label")?),
            "registrar" => return registrar::run().map(|_| ExitCode::SUCCESS),
            "--version" => {
                println!("omarchy-hud {VERSION}");
                return Ok(ExitCode::SUCCESS);
            }
            _ => {
                eprintln!("{USAGE}");
                return Ok(ExitCode::from(2));
            }
        }
    }

    let pid = match pid {
        Some(pid) => pid,
        None => focused_pid()?,
    };
    let bus = bus::Bus::session()?;
    let mut commands = menus::collect(&bus, pid);
    let app = recent::app_key(pid);
    recent::promote(&mut commands, |c| c.label.as_str(), &recent::labels(&app));

    if list {
        let mut out = std::io::stdout().lock();
        for c in &commands {
            let _ = if c.accel.is_empty() {
                writeln!(out, "{}", c.label)
            } else {
                writeln!(out, "{}\t{}", c.label, c.accel)
            };
        }
        return Ok(ExitCode::SUCCESS);
    }
    let label = match label {
        Some(label) => label,
        None if commands.is_empty() => {
            bus.notify("omarchy-hud", "This app exports no menu commands");
            return Ok(ExitCode::FAILURE);
        }
        None => match pick(&commands)? {
            Some(label) => {
                recent::remember(&app, &label);
                label
            }
            None => return Ok(ExitCode::SUCCESS),
        },
    };
    let command = commands
        .iter()
        .find(|c| c.label == label)
        .ok_or_else(|| format!("no command {label:?}"))?;
    menus::activate(&bus, &command.action)?;
    Ok(ExitCode::SUCCESS)
}

/// Asks Hyprland over its socket, which is faster than spawning hyprctl.
fn focused_pid() -> Result<u32, String> {
    let dir = env::var("XDG_RUNTIME_DIR").map_err(|_| "XDG_RUNTIME_DIR is not set")?;
    let sig = env::var("HYPRLAND_INSTANCE_SIGNATURE").map_err(|_| "not running under Hyprland")?;
    let mut sock = UnixStream::connect(format!("{dir}/hypr/{sig}/.socket.sock"))
        .map_err(|e| format!("hyprland socket: {e}"))?;
    sock.write_all(b"activewindow").map_err(|e| e.to_string())?;
    let mut reply = String::new();
    sock.read_to_string(&mut reply).map_err(|e| e.to_string())?;
    reply
        .lines()
        .find_map(|l| l.trim().strip_prefix("pid: "))
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| "no focused window".into())
}

/// Shows the commands in the Omarchy menu and returns the picked label.
///
/// shortcut: talks to omarchy-menu-select's private payload format (Menu.qml) to skip its
/// bash and perl setup (~45 ms); switch back to omarchy-menu-select if the payload changes.
fn pick(commands: &[menus::Command]) -> Result<Option<String>, String> {
    let dir = PathBuf::from(env::var("XDG_RUNTIME_DIR").map_err(|_| "XDG_RUNTIME_DIR is not set")?);
    let selection = dir.join(format!("omarchy-hud-{}.selection", std::process::id()));
    let done = dir.join(format!("omarchy-hud-{}.done", std::process::id()));
    let _ = (fs::remove_file(&selection), fs::remove_file(&done));

    // A leading tab means "no icon"; the accel shows as subtext and comes back after a tab.
    let options: Vec<String> = commands
        .iter()
        .map(|c| {
            if c.accel.is_empty() {
                format!("\t{}", c.label)
            } else {
                format!("\t{}\t{}", c.label, c.accel)
            }
        })
        .map(|o| json_string(&o))
        .collect();
    let payload = format!(
        r#"{{"mode":"select","prompt":"HUD","options":[{}],"multiple":false,"selected":[],"onChange":[],"changeKey":"","selectionFile":{},"doneFile":{},"width":800,"maxHeight":500}}"#,
        options.join(","),
        json_string(&selection.to_string_lossy()),
        json_string(&done.to_string_lossy()),
    );
    let status = Command::new("omarchy-shell")
        .args(["shell", "summon", "omarchy.menu", &payload])
        .stdout(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("omarchy-shell: {e}"))?;
    if !status.success() {
        return Err("omarchy-shell is not running".into());
    }
    while !done.exists() {
        thread::sleep(Duration::from_millis(5));
    }
    let picked = fs::read_to_string(&selection).ok();
    let _ = (fs::remove_file(&selection), fs::remove_file(&done));
    Ok(picked.and_then(|s| {
        s.trim_end_matches('\n')
            .split('\t')
            .next()
            .map(str::to_owned)
    }))
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn json_string_escapes() {
        assert_eq!(
            super::json_string("a\t\"b\"\\\u{1}"),
            r#""a\t\"b\"\\\u0001""#
        );
        assert_eq!(super::json_string("Plik › Zapisz"), "\"Plik › Zapisz\"");
    }
}
