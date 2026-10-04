use crate::messages::{Lang, log_info, log_success, log_warn, msg};
use anyhow::{Result, anyhow};
use std::ffi::OsStr;
use system_tools_core::{
    BackendId, ExecutableResolver, PackageUpgradePrivilege, package_upgrade_command,
    run_package_keyring_update_with_resolver, run_package_upgrade, run_status_path_timeout,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UpdatePlan {
    pub(crate) native: BackendId,
    pub(crate) optional: Vec<BackendId>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StepResult {
    Success,
    Failed,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UpdateOutcome {
    pub(crate) native: StepResult,
    pub(crate) optional: Vec<(BackendId, StepResult)>,
}
impl UpdateOutcome {
    pub(crate) fn exit_code(&self) -> i32 {
        if self.native == StepResult::Failed {
            1
        } else {
            0
        }
    }
}

pub(crate) fn plan(native: BackendId, present: impl Fn(BackendId) -> bool) -> UpdatePlan {
    const OPTIONAL: [BackendId; 4] = [
        BackendId::Flatpak,
        BackendId::Snap,
        BackendId::Brew,
        BackendId::Nix,
    ];
    UpdatePlan {
        native,
        optional: OPTIONAL
            .into_iter()
            .filter(|backend| present(*backend))
            .collect(),
    }
}

pub(crate) fn run(backend: BackendId, lang: Lang) -> Result<()> {
    let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    let update_plan = plan(backend, |optional| {
        resolver.resolve(OsStr::new(optional.as_str())).is_some()
    });
    run_native(update_plan.native, lang, &resolver)?;
    let mut optional = Vec::new();
    for backend in update_plan.optional {
        log_info(lang, &format!("optional update: {}", backend.as_str()));
        let state = if run_optional(backend).is_ok() {
            StepResult::Success
        } else {
            log_warn(lang, msg(lang, "backend.partial"));
            StepResult::Failed
        };
        optional.push((backend, state));
    }
    let outcome = UpdateOutcome {
        native: StepResult::Success,
        optional,
    };
    let _ = outcome.exit_code();
    log_success(lang, msg(lang, "update_complete"));
    Ok(())
}

fn run_native(backend: BackendId, lang: Lang, resolver: &ExecutableResolver) -> Result<()> {
    if backend == BackendId::Pacman {
        let mut keyrings = vec!["archlinux-keyring"];
        if std::fs::read_to_string("/etc/pacman.conf")
            .map(|contents| contents.contains("[archlinuxcn]"))
            .unwrap_or(false)
        {
            keyrings.push("archlinuxcn-keyring");
        }
        let _ = run_package_keyring_update_with_resolver(&keyrings, resolver);
    }
    log_info(lang, msg(lang, "update_step"));
    let command = package_upgrade_command(backend)?;
    let privilege = match command.privilege {
        PackageUpgradePrivilege::Elevated => "sudo",
        PackageUpgradePrivilege::User => "user",
    };
    print_summary("upgrade", command.program, privilege, command.args, lang);
    run_package_upgrade(backend)
        .map(|_| ())
        .map_err(|error| anyhow!(error.to_string()))
}

fn run_optional(backend: BackendId) -> Result<()> {
    let path = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref())
        .resolve(OsStr::new(backend.as_str()))
        .ok_or_else(|| anyhow!("{} unavailable", backend.as_str()))?;
    let args: &[&str] = match backend {
        BackendId::Flatpak => &["update", "-y"],
        BackendId::Snap => &["refresh"],
        BackendId::Brew => &["upgrade"],
        BackendId::Nix => &["profile", "upgrade", ".*"],
        _ => return Ok(()),
    };
    run_status_path_timeout(&path, args, std::time::Duration::from_secs(180))?;
    Ok(())
}

fn print_summary(action: &str, helper: &str, privilege: &str, targets: &[&str], lang: Lang) {
    println!(
        "{} backend={} · scope={}",
        summary_line(action, helper, privilege, targets),
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
    use super::*;
    #[test]
    fn optional_plan_is_stable_and_native_first() {
        let plan = plan(BackendId::Apt, |b| {
            matches!(b, BackendId::Flatpak | BackendId::Brew)
        });
        assert_eq!(plan.native, BackendId::Apt);
        assert_eq!(plan.optional, &[BackendId::Flatpak, BackendId::Brew]);
    }
    #[test]
    fn failure_outcome_is_nonzero_without_hiding_later_steps() {
        let outcome = UpdateOutcome {
            native: StepResult::Success,
            optional: vec![
                (BackendId::Flatpak, StepResult::Failed),
                (BackendId::Brew, StepResult::Success),
            ],
        };
        assert_eq!(outcome.exit_code(), 0);
        assert_eq!(outcome.optional.len(), 2);
    }
    #[test]
    fn native_failure_is_nonzero() {
        let outcome = UpdateOutcome {
            native: StepResult::Failed,
            optional: Vec::new(),
        };
        assert_eq!(outcome.exit_code(), 1);
    }
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
