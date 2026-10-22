//! omarchy-hud: search and run the menu commands of the focused window.

mod bus;
mod hypr;
mod labels;
mod menus;
mod recent;
mod registrar;
mod shortcuts;

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::Duration;
use std::{env, fs, thread};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const USAGE: &str =
    "usage: omarchy-hud [--pid PID | --window ADDRESS] [--list | --run LABEL] [--version]
       omarchy-hud registrar

Search and run the menu commands of the focused window.

  --pid PID    use this process instead of the focused window
  --window ADDRESS
               use this Hyprland window (as in hyprctl clients) instead of the focused one
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
    let (mut pid, mut window, mut list, mut label) = (None, None, false, None);
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
            "--window" => window = Some(args.next().ok_or("--window needs an address")?),
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

    let (pid, focus, window) = match pid {
        Some(pid) => (pid, None, None),
        None => {
            let (window, focus) = hypr::window(window.as_deref())?;
            (window.pid, focus, Some(window))
        }
    };
    let bus = bus::Bus::session()?;
    let mut commands = menus::collect(&bus, pid, focus);
    if let Some(window) = &window {
        for (label, accel) in shortcuts::for_class(&window.class) {
            if !commands.iter().any(|c| c.label == label) {
                commands.push(menus::Command {
                    label,
                    action: menus::Action::Shortcut {
                        window: window.address.clone(),
                        accel: accel.clone(),
                    },
                    accel,
                });
            }
        }
    }
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
            bus.notify(
                "omarchy-hud",
                "This app exports no menu commands and has no shortcut pack",
            );
            return Ok(ExitCode::FAILURE);
        }
        None => match pick(&commands, window.as_ref().and_then(hypr::rect_on_monitor))? {
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

/// The menu plugin from dist/menu/install: Omarchy's menu plus the `anchor` field.
const ANCHORED_MENU: &str = "omarchy-hud.menu";

fn anchored_menu_installed() -> bool {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .is_some_and(|c| {
            c.join("omarchy/plugins")
                .join(ANCHORED_MENU)
                .join("manifest.json")
                .exists()
        })
}

/// Shows the commands in the Omarchy menu, centered on `anchor` (the window's rectangle on
/// its monitor) when the anchored menu is installed, and returns the picked label.
///
/// shortcut: talks to omarchy-menu-select's private payload format (Menu.qml) to skip its
/// bash and perl setup (~45 ms); switch back to omarchy-menu-select if the payload changes.
fn pick(
    commands: &[menus::Command],
    anchor: Option<(i32, i32, i32, i32)>,
) -> Result<Option<String>, String> {
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
    let anchor = anchor
        .map(|(x, y, w, h)| format!(r#","anchor":{{"x":{x},"y":{y},"width":{w},"height":{h}}}"#))
        .unwrap_or_default();
    let payload = format!(
        r#"{{"mode":"select","prompt":"HUD","options":[{}],"multiple":false,"selected":[],"onChange":[],"changeKey":"","selectionFile":{},"doneFile":{},"width":800,"maxHeight":500{anchor}}}"#,
        options.join(","),
        json_string(&selection.to_string_lossy()),
        json_string(&done.to_string_lossy()),
    );
    let summon = |plugin: &str| {
        Command::new("omarchy-shell")
            .args(["shell", "summon", plugin, &payload])
            .stdout(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    if !(anchored_menu_installed() && summon(ANCHORED_MENU)) && !summon("omarchy.menu") {
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
