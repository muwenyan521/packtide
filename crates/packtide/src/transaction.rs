use anyhow::Result;
use std::io::IsTerminal;
use std::process::Command;
use system_tools_core::{
    BackendId, BackendRegistry, ExecutableResolver, PackageBackend, PackageId, PackageIdentity,
    PackageScope, TransactionAction, command_exists, run_command_plan,
};

pub(crate) fn execute_package(
    helper: &str,
    action: TransactionAction,
    packages: &[String],
) -> Result<()> {
    let backend = if helper == "yay" {
        BackendId::Yay
    } else {
        BackendId::Paru
    };
    let ids = packages
        .iter()
        .map(|name| {
            PackageIdentity::new(
                backend,
                system_tools_core::PackageKind::Aur,
                PackageScope::User,
                PackageId::new(name.clone()).expect("package id"),
            )
        })
        .collect::<Vec<_>>();
    execute_package_typed(helper, action, &ids)
}

pub(crate) fn execute_package_typed(
    helper: &str,
    action: TransactionAction,
    packages: &[PackageIdentity],
) -> Result<()> {
    let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    let registry = BackendRegistry::default();
    for backend in [BackendId::Pacman, BackendId::Paru, BackendId::Yay] {
        let selected = packages
            .iter()
            .filter(|package| package.backend == backend)
            .cloned()
            .collect::<Vec<_>>();
        if selected.is_empty() {
            continue;
        }
        let helper_name = match backend {
            BackendId::Pacman => "pacman",
            BackendId::Paru => "paru",
            BackendId::Yay => "yay",
            _ => unreachable!(),
        };
        if backend != BackendId::Pacman && helper_name != helper {
            anyhow::bail!(
                "{}: package identity requires {helper_name}, configured helper is {helper}",
                crate::locale::text(crate::locale::current(), "backend.unsupported", &[])
            );
        }
        let plan = registry
            .backend(backend)
            .expect("builtin backend")
            .write_with_resolver(
                system_tools_core::WriteOperation::transaction(action, selected.clone()),
                &resolver,
            )?;
        let targets = selected
            .iter()
            .map(|p| p.native_key.as_str())
            .collect::<Vec<_>>();
        print_summary(
            action.as_str(),
            helper_name,
            if backend == BackendId::Pacman {
                "direct"
            } else {
                "helper-managed"
            },
            &targets,
        );
        run_command_plan(&plan.command)?;
    }
    Ok(())
}

pub(crate) fn execute_flatpak(packages: &[PackageIdentity]) -> Result<()> {
    let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    let plan = BackendRegistry::default()
        .backend(BackendId::Flatpak)
        .expect("builtin backend")
        .write_with_resolver(
            system_tools_core::WriteOperation::transaction(
                TransactionAction::Remove,
                packages.to_vec(),
            ),
            &resolver,
        )?;
    let targets = packages
        .iter()
        .map(|p| p.native_key.as_str())
        .collect::<Vec<_>>();
    let scope = packages
        .first()
        .map_or(PackageScope::User, |package| package.scope);
    let privilege = if scope == PackageScope::System {
        "elevated"
    } else {
        "direct"
    };
    print_summary_with_scope("remove", "flatpak", privilege, &targets, scope);
    run_command_plan(&plan.command)?;
    Ok(())
}

pub(crate) fn print_summary(action: &str, helper: &str, privilege: &str, targets: &[&str]) {
    let scope = if helper == "pacman" {
        PackageScope::System
    } else {
        PackageScope::User
    };
    print_summary_with_scope(action, helper, privilege, targets, scope);
}

fn print_summary_with_scope(
    action: &str,
    helper: &str,
    privilege: &str,
    targets: &[&str],
    scope: PackageScope,
) {
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
    let scope = crate::locale::scope_label(lang, scope);
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
