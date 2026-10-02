use anyhow::Result;
use std::io::IsTerminal;
use std::process::Command;
use system_tools_core::{
    BackendId, BuiltinBackend, PackageBackend, PackageId, PackageScope, TransactionAction,
    command_exists, run_status_path,
};

pub(crate) fn execute_package(
    helper: &str,
    action: TransactionAction,
    packages: &[String],
) -> Result<()> {
    let backend = match helper {
        "paru" => BackendId::Paru,
        "yay" => BackendId::Yay,
        _ => anyhow::bail!(
            "{}: unsupported AUR helper: {helper}",
            crate::locale::text(crate::locale::current(), "backend.unsupported", &[])
        ),
    };
    let package_ids = packages
        .iter()
        .cloned()
        .map(PackageId::new)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let plan = BuiltinBackend::new(backend).transaction(action, package_ids)?;
    let targets = packages.iter().map(String::as_str).collect::<Vec<_>>();
    print_summary(action.as_str(), helper, "helper-managed", &targets);
    run_status_path(&plan.command.program, &plan.command.args)?;
    Ok(())
}

pub(crate) fn execute_flatpak(packages: &[String]) -> Result<()> {
    let package_ids = packages
        .iter()
        .cloned()
        .map(PackageId::new)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let plan = BuiltinBackend::new(BackendId::Flatpak)
        .transaction(TransactionAction::Remove, package_ids)?;
    let targets = packages.iter().map(String::as_str).collect::<Vec<_>>();
    print_summary("remove", "flatpak", "direct", &targets);
    run_status_path(&plan.command.program, &plan.command.args)?;
    Ok(())
}

pub(crate) fn print_summary(action: &str, helper: &str, privilege: &str, targets: &[&str]) {
    let summary = summary_line(action, helper, privilege, targets);
    let lang = crate::locale::current();
    let backend = match helper {
        "pacman" => format!(
            "{} · {}",
            crate::locale::backend_label(lang, BackendId::Pacman),
            crate::locale::text(lang, "backend.native", &[])
        ),
        "flatpak" => crate::locale::source_label(lang, system_tools_core::PackageSource::Flatpak),
        _ => crate::locale::backend_label(lang, BackendId::Paru),
    };
    let scope = if helper == "pacman" {
        crate::locale::scope_label(lang, PackageScope::System)
    } else {
        crate::locale::scope_label(lang, PackageScope::User)
    };
    let summary = format!("{summary} backend={backend} scope={scope}");
    if std::io::stdout().is_terminal()
        && let Some((label, rest)) = summary.split_once(' ')
    {
        println!("\x1b[1;34m{label}\x1b[0m {rest}");
    } else {
        println!("{summary}");
    }
}

fn summary_line(action: &str, helper: &str, privilege: &str, targets: &[&str]) -> String {
    format!(
        "transaction: action={action} helper={helper} privilege={privilege} targets={}",
        targets.join(",")
    )
}

pub(crate) fn refresh_waybar_cache() {
    if command_exists("pkill") {
        let _ = Command::new("pkill")
            .args(["-SIGUSR1", "-f", "check-updates.sh"])
            .status();
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    if command_exists("flock") {
        let _ = Command::new("flock")
            .args(["-x", "-w", "120", "/tmp/waybar-updates.lock", "true"])
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::summary_line;

    #[test]
    fn renders_transaction_summary_with_explicit_privilege_boundary() {
        assert_eq!(
            summary_line(
                "install",
                "paru",
                "helper-managed",
                &["core/bash", "aur/tool"]
            ),
            "transaction: action=install helper=paru privilege=helper-managed targets=core/bash,aur/tool"
        );
    }
}
