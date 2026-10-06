use crate::messages::{Lang, log_info, log_success, log_warn, msg};
use anyhow::{Result, anyhow};
use std::ffi::OsStr;
use system_tools_core::{
    BackendId, CommandPlan, ExecutableResolver, PackageUpgradePrivilege, package_upgrade_command,
    run_command_plan, run_package_keyring_update_with_resolver, run_package_upgrade_with_resolver,
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
    fn native_succeeded(&self) -> bool {
        self.native == StepResult::Success
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
    let outcome = execute_plan(
        &update_plan,
        || run_native(update_plan.native, lang, &resolver),
        |backend| {
            log_info(lang, &format!("optional update: {}", backend.as_str()));
            run_optional(backend, &resolver).map_err(|error| {
                log_warn(lang, &format!("{}: {error}", backend.as_str()));
                error
            })
        },
    );
    for (backend, state) in &outcome.optional {
        if *state == StepResult::Failed {
            log_warn(
                lang,
                &format!("{}: {}", backend.as_str(), msg(lang, "backend.partial")),
            );
        }
    }
    if !outcome.native_succeeded() {
        return Err(anyhow!("native package update failed"));
    }
    if outcome
        .optional
        .iter()
        .all(|(_, state)| *state == StepResult::Success)
    {
        log_success(lang, msg(lang, "update_complete"));
    }
    Ok(())
}

fn execute_plan(
    plan: &UpdatePlan,
    native: impl FnOnce() -> Result<()>,
    mut run_optional: impl FnMut(BackendId) -> Result<()>,
) -> UpdateOutcome {
    let native = if native().is_ok() {
        StepResult::Success
    } else {
        return UpdateOutcome {
            native: StepResult::Failed,
            optional: Vec::new(),
        };
    };
    let mut optional = Vec::new();
    for &backend in &plan.optional {
        let state = if run_optional(backend).is_ok() {
            StepResult::Success
        } else {
            StepResult::Failed
        };
        optional.push((backend, state));
    }
    UpdateOutcome { native, optional }
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
    run_package_upgrade_with_resolver(backend, resolver)
        .map(|_| ())
        .map_err(|error| anyhow!(error.to_string()))
}

fn run_optional(backend: BackendId, resolver: &ExecutableResolver) -> Result<()> {
    let command = package_upgrade_command(backend)?;
    let program = resolver
        .resolve(OsStr::new(command.program))
        .ok_or_else(|| anyhow!("{} unavailable", command.program))?;
    let mut plan = CommandPlan::new(program)
        .with_backend(backend)
        .with_env_remove("LD_PRELOAD")
        .with_env_remove("LD_LIBRARY_PATH")
        .with_locale("C")
        .with_privilege(command.privilege);
    plan.args
        .extend(command.args.iter().map(|arg| (*arg).into()));
    if backend == BackendId::Flatpak {
        plan.args.push("-y".into());
    }
    run_command_plan(&plan).map(|_| ())
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
    fn optional_failure_preserves_native_success_and_later_steps() {
        let outcome = UpdateOutcome {
            native: StepResult::Success,
            optional: vec![
                (BackendId::Flatpak, StepResult::Failed),
                (BackendId::Brew, StepResult::Success),
            ],
        };
        assert!(outcome.native_succeeded());
        assert_eq!(outcome.optional.len(), 2);
    }
    #[test]
    fn native_failure_is_nonzero() {
        let outcome = UpdateOutcome {
            native: StepResult::Failed,
            optional: Vec::new(),
        };
        assert!(!outcome.native_succeeded());
    }
    fn command_fixture(native_status: i32) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!(
            "systide-update-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir(&path).expect("create command fixture");
        for (program, status) in [
            ("brew", native_status),
            ("paru", native_status),
            ("flatpak", 7),
            ("snap", 0),
        ] {
            let file = path.join(program);
            std::fs::write(
                &file,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$*\" > '{}.argv'\nexit {status}\n",
                    file.display()
                ),
            )
            .expect("write command");
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))
                .expect("chmod command");
        }
        path
    }
    #[test]
    fn native_command_failure_short_circuits_optional_commands() {
        let path = command_fixture(9);
        let resolver = ExecutableResolver::from_path(Some(path.as_os_str()));
        let plan = UpdatePlan {
            native: BackendId::Brew,
            optional: vec![BackendId::Flatpak, BackendId::Snap],
        };
        let outcome = execute_plan(
            &plan,
            || run_native(plan.native, Lang::En, &resolver),
            |backend| run_optional(backend, &resolver),
        );
        assert_eq!(outcome.native, StepResult::Failed);
        assert!(outcome.optional.is_empty());
        assert!(path.join("brew.argv").exists());
        assert!(!path.join("flatpak.argv").exists());
        assert!(!path.join("snap.argv").exists());
        println!("native exit=9; flatpak invoked=false; snap invoked=false");
        std::fs::remove_dir_all(path).expect("remove command fixture");
    }
    #[test]
    fn optional_command_failure_is_recorded_and_later_command_runs() {
        let path = command_fixture(0);
        let resolver = ExecutableResolver::from_path(Some(path.as_os_str()));
        let plan = UpdatePlan {
            native: BackendId::Brew,
            optional: vec![BackendId::Flatpak, BackendId::Snap],
        };
        let outcome = execute_plan(
            &plan,
            || run_native(plan.native, Lang::En, &resolver),
            |backend| run_optional(backend, &resolver),
        );
        assert_eq!(
            outcome.optional,
            vec![
                (BackendId::Flatpak, StepResult::Failed),
                (BackendId::Snap, StepResult::Success)
            ]
        );
        assert!(outcome.native_succeeded());
        println!(
            "optional outcome={:?}; exit={}",
            outcome.optional,
            if outcome.native == StepResult::Failed {
                1
            } else {
                0
            }
        );
        assert_eq!(
            std::fs::read_to_string(path.join("flatpak.argv")).expect("flatpak command ran"),
            "update -y\n"
        );
        assert_eq!(
            std::fs::read_to_string(path.join("snap.argv")).expect("later snap command ran"),
            "refresh\n"
        );
        std::fs::remove_dir_all(path).expect("remove command fixture");
    }
    #[test]
    fn run_consumes_partial_outcome_without_claiming_full_success() {
        const CHILD: &str = "SYSTIDE_PARTIAL_UPDATE_CHILD";
        if std::env::var_os(CHILD).is_some() {
            assert!(run(BackendId::Paru, Lang::En).is_ok());
            return;
        }
        let path = command_fixture(0);
        let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "update::tests::run_consumes_partial_outcome_without_claiming_full_success",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("PATH", &path)
            .output()
            .expect("run update subprocess");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "{output:?}");
        assert!(stdout.contains("flatpak:"));
        assert!(stdout.contains(msg(Lang::En, "backend.partial")));
        assert!(!stdout.contains(msg(Lang::En, "update_complete")));
        assert_eq!(
            std::fs::read_to_string(path.join("snap.argv")).expect("later optional ran"),
            "refresh\n"
        );
        println!("{stdout}");
        std::fs::remove_dir_all(path).expect("remove command fixture");
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
