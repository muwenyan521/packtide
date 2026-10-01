use anyhow::{Result, bail};
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use system_tools_core::{command_exists, run_capture, run_privileged, run_status_no_args};

use crate::messages::{Lang, log_info, msg};

pub(crate) fn check_age(lang: Lang) -> Result<()> {
    if !is_arch() {
        return Ok(());
    }
    log_info(lang, msg(lang, "mirror_step"));
    let path = Path::new("/etc/pacman.d/mirrorlist");
    let text = fs::read_to_string(path).unwrap_or_default();
    let Some(when) = text.lines().find_map(|line| line.strip_prefix("# When:")) else {
        return Ok(());
    };
    let Ok(timestamp) = parse_date_timestamp(when.trim()) else {
        return Ok(());
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let age = now.saturating_sub(timestamp) / 86_400;
    if age > 30 {
        println!("{} ({age} days)", msg(lang, "mirror_old"));
        print!("{}", msg(lang, "mirror_confirm"));
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().lock().read_line(&mut answer)?;
        if answer.trim().is_empty() || answer.trim_start().starts_with(['y', 'Y']) {
            let mirror =
                env::var_os("HOME").map(|home| Path::new(&home).join(".local/bin/mirror-update"));
            if let Some(mirror) = mirror.filter(|path| path.is_file()) {
                run_status_no_args(mirror.as_os_str())?;
            } else if command_exists("reflector") {
                run_privileged(&[
                    "reflector",
                    "--protocol",
                    "https",
                    "--sort",
                    "rate",
                    "--latest",
                    "50",
                    "--fastest",
                    "10",
                ])?;
            } else {
                println!("{}", msg(lang, "mirror_skip"));
            }
        }
    } else {
        println!("{} ({age} days)", msg(lang, "mirror_fresh"));
    }
    Ok(())
}

fn parse_date_timestamp(value: &str) -> Result<u64> {
    let output = run_capture("date", &["-u", "-d", value, "+%s"], true)?;
    if !output.status.success() {
        bail!("invalid date")
    }
    Ok(output.stdout.trim().parse()?)
}

fn is_arch() -> bool {
    fs::read_to_string("/etc/os-release")
        .map(|v| {
            v.lines()
                .any(|line| line == "ID=arch" || line == "ID_LIKE=arch")
        })
        .unwrap_or(false)
}
