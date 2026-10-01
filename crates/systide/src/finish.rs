use anyhow::Result;
use std::ffi::OsStr;
use system_tools_core::{ExecutableResolver, run_capture_path, run_privileged, run_status_path};

use crate::messages::{Lang, log_info, log_success, log_warn, msg};

pub(crate) fn run(lang: Lang) -> Result<()> {
    let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    if let Some(flatpak) = resolver.resolve(OsStr::new("flatpak")) {
        log_info(lang, msg(lang, "flatpak_step"));
        run_status_path(&flatpak, &["update", "-y"])?;
    } else {
        log_warn(lang, msg(lang, "flatpak_skip"));
    }
    if resolver.resolve(OsStr::new("grub-mkconfig")).is_some() {
        log_info(lang, msg(lang, "grub_step"));
        if run_privileged(&["grub-mkconfig", "-o", "/boot/grub/grub.cfg"]).is_ok() {
            log_success(lang, msg(lang, "grub_ok"));
        } else {
            log_warn(lang, msg(lang, "grub_fail"));
        }
    } else {
        log_warn(lang, msg(lang, "grub_skip"));
    }
    signal_waybar();
    Ok(())
}

fn signal_waybar() {
    let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    if let Some(pkill) = resolver.resolve(OsStr::new("pkill")) {
        let _ = run_capture_path(&pkill, &["-SIGUSR1", "-f", "check-updates.sh"], true);
    }
}
