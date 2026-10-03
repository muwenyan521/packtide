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
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};
    use system_tools_core::BackendId;

    #[test]
    fn finish_hooks_do_not_run_for_non_arch_or_optional_updates() {
        assert!(!arch_only_hooks(Some(BackendId::Apt)));
        assert!(!arch_only_hooks(Some(BackendId::Flatpak)));
        assert!(arch_only_hooks(Some(BackendId::Pacman)));
    }

    #[test]
    fn flatpak_is_updated_once_when_finish_follows_optional_update() {
        const CHILD_ENV: &str = "SYSTIDE_FINISH_REGRESSION_CHILD";
        if std::env::var_os(CHILD_ENV).is_some() {
            crate::update::run(BackendId::Paru, Lang::En).expect("run native and optional updates");
            run(Lang::En).expect("finish completed updates");
            return;
        }

        // Given an isolated user helper and a Flatpak command that records every invocation.
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let fixture =
            std::env::temp_dir().join(format!("systide-finish-{}-{nonce}", std::process::id()));
        fs::create_dir(&fixture).expect("create isolated PATH fixture");
        let calls = fixture.join("flatpak-calls");
        for (name, body) in [
            ("paru", "#!/bin/sh\nexit 0\n"),
            (
                "flatpak",
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$SYSTIDE_FLATPAK_CALLS\"\n",
            ),
        ] {
            let path = fixture.join(name);
            fs::write(&path, body).expect("write fixture executable");
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))
                .expect("make fixture executable");
        }

        // When the real update and finish phases run in a child with that PATH.
        let output = Command::new(std::env::current_exe().expect("locate test binary"))
            .args([
                "--exact",
                "finish::tests::flatpak_is_updated_once_when_finish_follows_optional_update",
                "--nocapture",
            ])
            .env(CHILD_ENV, "1")
            .env("PATH", &fixture)
            .env("SYSTIDE_FLATPAK_CALLS", &calls)
            .output()
            .expect("run isolated update and finish phases");
        let recorded = fs::read_to_string(&calls).expect("read Flatpak invocations");
        fs::remove_dir_all(&fixture).expect("remove isolated PATH fixture");

        // Then Flatpak receives one update command, not a second finish-stage transaction.
        assert!(output.status.success(), "{output:?}");
        assert_eq!(recorded.lines().collect::<Vec<_>>(), ["update -y"]);
    }
}
