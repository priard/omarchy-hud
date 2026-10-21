//! Benchmarks command collection: wall time, CPU and peak RAM per run, from rusage.
//! Written in Rust so the parent stays small: a forked child inherits the parent's
//! peak RSS, which would hide a small binary's real footprint.
//!
//! usage: cargo run --release --example bench -- RUNS PID COMMAND [COMMAND...]
//! Each COMMAND runs as `COMMAND --pid PID --list`.

use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::Instant;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (runs, pid) = (args[0].parse::<usize>().expect("RUNS"), &args[1]);
    for command in &args[2..] {
        let (mut wall, mut cpu, mut rss) = (vec![], vec![], vec![]);
        for _ in 0..runs {
            let start = Instant::now();
            let child = Command::new(command).args(["--pid", pid, "--list"]).stdout(Stdio::null()).process_group(0).spawn().expect("spawn");
            let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
            let mut status = 0;
            unsafe { libc::wait4(child.id() as i32, &mut status, 0, &mut usage) };
            wall.push(start.elapsed().as_secs_f64() * 1000.0);
            let ms = |t: libc::timeval| t.tv_sec as f64 * 1000.0 + t.tv_usec as f64 / 1000.0;
            cpu.push(ms(usage.ru_utime) + ms(usage.ru_stime));
            rss.push(usage.ru_maxrss as f64 / 1024.0);
        }
        println!(
            "{command}: wall {:.1} ms, cpu {:.1} ms, peak RSS {:.1} MB (median of {runs})",
            median(wall),
            median(cpu),
            median(rss)
        );
    }
}
