use std::fs;
use system_tools_core::{
    BackendId, PackageUpgradePrivilege, package_upgrade_command, run_package_upgrade,
    run_privileged,
};

use crate::messages::{Lang, log_error, log_info, log_success, log_warn, msg};

pub(crate) fn run(backend: BackendId, lang: Lang) -> anyhow::Result<()> {
    if matches!(
        backend,
        BackendId::Pacman | BackendId::Paru | BackendId::Yay
    ) {
        let mut keyrings = vec!["archlinux-keyring"];
        if fs::read_to_string("/etc/pacman.conf")
            .map(|contents| contents.contains("[archlinuxcn]"))
            .unwrap_or(false)
        {
            keyrings.push("archlinuxcn-keyring");
        }
        let mut args = vec!["pacman", "-Sy", "--needed", "--noconfirm"];
        args.extend(keyrings);
        print_summary("keyring", "pacman", "sudo", &args[3..], lang);
        if run_privileged(&args).is_ok() {
            log_success(lang, msg(lang, "keyring_ok"));
        } else {
            log_warn(lang, msg(lang, "keyring_warn"));
        }
    }
    log_info(lang, msg(lang, "update_step"));
    let command = package_upgrade_command(backend)?;
    let privilege = match command.privilege {
        PackageUpgradePrivilege::Elevated => "sudo",
        PackageUpgradePrivilege::User => "helper-managed",
    };
    print_summary("upgrade", command.program, privilege, command.args, lang);
    let result = run_package_upgrade(backend);
    if result.is_err() {
        log_error(lang, msg(lang, "backend.partial"));
        log_error(lang, msg(lang, "partial_title"));
        log_error(lang, msg(lang, "partial_database"));
        log_error(lang, msg(lang, "partial_stop"));
        log_error(lang, msg(lang, "partial_retry"));
        return result.map(|_| ()).map_err(anyhow::Error::msg);
    }
    log_success(lang, msg(lang, "update_complete"));
    Ok(())
}

fn print_summary(action: &str, helper: &str, privilege: &str, targets: &[&str], lang: Lang) {
    println!(
        "{} backend={} · {} scope={}",
        summary_line(action, helper, privilege, targets),
        msg(lang, "backend.pacman"),
        msg(lang, "backend.native"),
        msg(lang, "scope.system")
    );
}

fn summary_line(action: &str, helper: &str, privilege: &str, targets: &[&str]) -> String {
    format!(
        "transaction: action={action} helper={helper} privilege={privilege} targets={}",
        targets.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::summary_line;

    #[test]
    fn renders_upgrade_summary_with_helper_privilege() {
        assert_eq!(
            summary_line(
                "upgrade",
                "paru",
                "helper-managed",
                &["-Su", "--skipreview"]
            ),
            "transaction: action=upgrade helper=paru privilege=helper-managed targets=-Su,--skipreview"
        );
    }
}
