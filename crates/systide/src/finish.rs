use anyhow::Result;
use std::ffi::OsStr;
use system_tools_core::{BackendId, ExecutableResolver, run_capture_path, run_privileged};

use crate::messages::{Lang, log_info, log_success, log_warn, msg};

pub(crate) fn run(lang: Lang) -> Result<()> {
    let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    if arch_only_hooks(crate::operations::detect_manager(lang).ok())
        && resolver.resolve(OsStr::new("grub-mkconfig")).is_some()
    {
        log_info(lang, msg(lang, "grub_step"));
        if run_privileged(&["grub-mkconfig", "-o", "/boot/grub/grub.cfg"]).is_ok() {
            log_success(lang, msg(lang, "grub_ok"));
        } else {
            log_warn(lang, msg(lang, "grub_fail"));
        }
    } else if arch_only_hooks(crate::operations::detect_manager(lang).ok()) {
        log_warn(lang, msg(lang, "grub_skip"));
    }
    signal_waybar();
    log_success(lang, msg(lang, "finish_complete"));
    Ok(())
}

fn arch_only_hooks(backend: Option<BackendId>) -> bool {
    backend == Some(BackendId::Pacman)
}

fn signal_waybar() {
    let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    if let Some(pkill) = resolver.resolve(OsStr::new("pkill")) {
        let _ = run_capture_path(&pkill, &["-SIGUSR1", "-f", "check-updates.sh"], true);
    }
}

#[cfg(test)]
mod tests {
    use super::arch_only_hooks;
    use system_tools_core::BackendId;

    #[test]
    fn finish_hooks_do_not_run_for_non_arch_or_optional_updates() {
        assert!(!arch_only_hooks(Some(BackendId::Apt)));
        assert!(!arch_only_hooks(Some(BackendId::Flatpak)));
        assert!(arch_only_hooks(Some(BackendId::Pacman)));
    }
}
