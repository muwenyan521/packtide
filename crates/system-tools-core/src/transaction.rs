use std::ffi::OsStr;
use std::process::ExitStatus;

use crate::{BackendError, BackendId, CommandPlan, ExecutableResolver, run_command_plan};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CommandPrivilege {
    Elevated,
    User,
}

pub type PackageUpgradePrivilege = CommandPrivilege;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageUpgradeCommand<'a> {
    pub program: &'a str,
    pub args: &'static [&'static str],
    pub privilege: PackageUpgradePrivilege,
}

pub fn package_upgrade_command(
    backend: BackendId,
) -> Result<PackageUpgradeCommand<'static>, BackendError> {
    let command = match backend {
        BackendId::Pacman => PackageUpgradeCommand {
            program: "pacman",
            args: &["-Su"],
            privilege: PackageUpgradePrivilege::Elevated,
        },
        BackendId::Paru => PackageUpgradeCommand {
            program: "paru",
            args: &["-Su", "--skipreview"],
            privilege: PackageUpgradePrivilege::User,
        },
        BackendId::Yay => PackageUpgradeCommand {
            program: "yay",
            args: &["-Su", "--answeredit", "None"],
            privilege: PackageUpgradePrivilege::User,
        },
        BackendId::Flatpak => PackageUpgradeCommand {
            program: "flatpak",
            args: &["update"],
            privilege: PackageUpgradePrivilege::User,
        },
        BackendId::Apt => PackageUpgradeCommand {
            program: "apt-get",
            args: &["upgrade", "-y"],
            privilege: PackageUpgradePrivilege::Elevated,
        },
        BackendId::Dnf5 => PackageUpgradeCommand {
            program: "dnf5",
            args: &["upgrade", "-y"],
            privilege: PackageUpgradePrivilege::Elevated,
        },
        BackendId::Dnf4 => PackageUpgradeCommand {
            program: "dnf",
            args: &["upgrade", "-y"],
            privilege: PackageUpgradePrivilege::Elevated,
        },
        BackendId::Zypper => PackageUpgradeCommand {
            program: "zypper",
            args: &["update", "-y"],
            privilege: PackageUpgradePrivilege::Elevated,
        },
        BackendId::Apk => PackageUpgradeCommand {
            program: "apk",
            args: &["upgrade"],
            privilege: PackageUpgradePrivilege::Elevated,
        },
        BackendId::Xbps => PackageUpgradeCommand {
            program: "xbps-install",
            args: &["-Su"],
            privilege: PackageUpgradePrivilege::Elevated,
        },
        BackendId::Snap => PackageUpgradeCommand {
            program: "snap",
            args: &["refresh"],
            privilege: PackageUpgradePrivilege::Elevated,
        },
        BackendId::Brew => PackageUpgradeCommand {
            program: "brew",
            args: &["upgrade"],
            privilege: PackageUpgradePrivilege::User,
        },
        BackendId::Nix => PackageUpgradeCommand {
            program: "nix",
            args: &[
                "--extra-experimental-features",
                "nix-command flakes",
                "profile",
                "upgrade",
                ".*",
            ],
            privilege: PackageUpgradePrivilege::User,
        },
    };
    Ok(command)
}

pub fn package_keyring_plan(
    keyrings: &[&str],
    resolver: &ExecutableResolver,
) -> Result<CommandPlan, BackendError> {
    let program =
        resolver
            .resolve(OsStr::new("pacman"))
            .ok_or_else(|| BackendError::CommandUnavailable {
                backend: BackendId::Pacman,
                operation: "update package keyrings",
                command: "pacman".to_owned(),
            })?;
    let mut plan = CommandPlan::new(program)
        .with_backend(BackendId::Pacman)
        .with_env_remove("LD_PRELOAD")
        .with_env_remove("LD_LIBRARY_PATH")
        .with_locale("C")
        .with_privilege(CommandPrivilege::Elevated);
    plan.args.extend(
        ["-Sy", "--needed", "--noconfirm"]
            .into_iter()
            .map(OsStr::new)
            .map(ToOwned::to_owned),
    );
    plan.args.extend(
        keyrings
            .iter()
            .map(|keyring| OsStr::new(keyring).to_owned()),
    );
    Ok(plan)
}

pub fn run_package_keyring_update_with_resolver(
    keyrings: &[&str],
    resolver: &ExecutableResolver,
) -> Result<ExitStatus, BackendError> {
    let plan = package_keyring_plan(keyrings, resolver)?;
    run_command_plan(&plan).map_err(|error| BackendError::CommandFailed {
        backend: BackendId::Pacman,
        operation: "update package keyrings",
        message: error.to_string(),
    })
}

pub fn run_package_upgrade(backend: BackendId) -> Result<ExitStatus, BackendError> {
    let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    run_package_upgrade_with_resolver(backend, &resolver)
}

pub fn run_package_upgrade_with_resolver(
    backend: BackendId,
    resolver: &ExecutableResolver,
) -> Result<ExitStatus, BackendError> {
    let command = package_upgrade_command(backend)?;
    let program = resolver
        .resolve(OsStr::new(command.program))
        .ok_or_else(|| BackendError::CommandUnavailable {
            backend,
            operation: "upgrade packages",
            command: command.program.to_owned(),
        })?;
    let mut plan = CommandPlan::new(program)
        .with_backend(backend)
        .with_env_remove("LD_PRELOAD")
        .with_env_remove("LD_LIBRARY_PATH")
        .with_locale("C")
        .with_privilege(command.privilege);
    plan.args
        .extend(command.args.iter().map(|arg| (*arg).into()));
    run_command_plan(&plan).map_err(|error| BackendError::CommandFailed {
        backend,
        operation: "upgrade packages",
        message: error.to_string(),
    })
}

/// Builds the upgrade command from a typed backend identifier.
pub fn package_upgrade_command_for(
    backend: crate::BackendId,
) -> Result<PackageUpgradeCommand<'static>, BackendError> {
    package_upgrade_command(backend)
}

#[cfg(test)]
mod tests {
    use super::{
        PackageUpgradePrivilege, package_upgrade_command, package_upgrade_command_for,
        run_package_upgrade, run_package_upgrade_with_resolver,
    };
    use crate::BackendId;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn package_upgrade_command_uses_privileged_pacman_argv() {
        // Given pacman as the selected package manager
        // When the shared command spec is built
        let command = package_upgrade_command(BackendId::Pacman).expect("pacman is supported");

        // Then the invocation is elevated and targets a system upgrade
        assert_eq!(command.program, "pacman");
        assert_eq!(command.args, ["-Su"]);
        assert_eq!(command.privilege, PackageUpgradePrivilege::Elevated);
    }

    #[test]
    fn package_upgrade_command_preserves_paru_review_flags() {
        // Given paru as the selected package manager
        // When the shared command spec is built
        let command = package_upgrade_command(BackendId::Paru).expect("paru is supported");

        // Then paru remains a direct command with review disabled
        assert_eq!(command.program, "paru");
        assert_eq!(command.args, ["-Su", "--skipreview"]);
        assert_eq!(command.privilege, PackageUpgradePrivilege::User);
    }

    #[test]
    fn package_upgrade_command_preserves_yay_edit_answer() {
        // Given yay as the selected package manager
        // When the shared command spec is built
        let command = package_upgrade_command(BackendId::Yay).expect("yay is supported");

        // Then yay remains a direct command with edit prompts disabled
        assert_eq!(command.program, "yay");
        assert_eq!(command.args, ["-Su", "--answeredit", "None"]);
        assert_eq!(command.privilege, PackageUpgradePrivilege::User);
    }

    #[test]
    fn package_upgrade_command_uses_matching_dnf_generation_executable() {
        let dnf5 = package_upgrade_command(BackendId::Dnf5).expect("dnf5 is supported");
        assert_eq!(dnf5.program, "dnf5");
        assert_eq!(dnf5.args, ["upgrade", "-y"]);
        assert_eq!(dnf5.privilege, PackageUpgradePrivilege::Elevated);

        let dnf4 = package_upgrade_command(BackendId::Dnf4).expect("dnf4 is supported");
        assert_eq!(dnf4.program, "dnf");
        assert_eq!(dnf4.args, ["upgrade", "-y"]);
        assert_eq!(dnf4.privilege, PackageUpgradePrivilege::Elevated);
    }

    #[test]
    fn package_upgrade_command_runs_snap_refresh_elevated() {
        let command = package_upgrade_command(BackendId::Snap).expect("snap is supported");
        assert_eq!(command.program, "snap");
        assert_eq!(command.args, ["refresh"]);
        assert_eq!(command.privilege, PackageUpgradePrivilege::Elevated);
    }

    #[test]
    fn package_upgrade_command_enables_nix_command_features() {
        let command = package_upgrade_command(BackendId::Nix).expect("nix is supported");
        assert_eq!(
            command.args,
            [
                "--extra-experimental-features",
                "nix-command flakes",
                "profile",
                "upgrade",
                ".*"
            ]
        );
    }

    #[test]
    fn typed_upgrade_command_rejects_unimplemented_backends_without_sentinel() {
        for backend in [
            BackendId::Apt,
            BackendId::Dnf5,
            BackendId::Dnf4,
            BackendId::Zypper,
            BackendId::Apk,
            BackendId::Xbps,
            BackendId::Snap,
            BackendId::Brew,
            BackendId::Nix,
        ] {
            assert!(package_upgrade_command_for(backend).is_ok());
        }
    }

    #[test]
    fn typed_upgrade_rejects_unsupported_backend_before_spawn() {
        let _ = run_package_upgrade(BackendId::Apt).expect_err("missing apt-get fixture");
    }

    #[test]
    fn upgrade_plan_resolves_absolute_user_command_and_applies_locale() {
        let root =
            std::env::temp_dir().join(format!("system-tools-core-upgrade-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create fixture directory");
        let executable = root.join("paru");
        let capture = root.join("capture");
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$0\" \"$@\" \"$LC_ALL\" > '{}'\n",
                capture.display()
            ),
        )
        .expect("create fake helper");
        let mut permissions = fs::metadata(&executable)
            .expect("stat fake helper")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("make fake helper executable");
        let resolver = crate::ExecutableResolver::from_path(Some(root.as_os_str()));

        run_package_upgrade_with_resolver(BackendId::Paru, &resolver)
            .expect("fake helper upgrade succeeds");
        let lines = fs::read_to_string(&capture).expect("read captured plan invocation");
        let mut lines = lines.lines();
        assert_eq!(lines.next(), Some(executable.to_str().expect("utf8 path")));
        assert_eq!(lines.next(), Some("-Su"));
        assert_eq!(lines.next(), Some("--skipreview"));
        assert_eq!(lines.next(), Some("C"));
        assert_eq!(lines.next(), None);
        fs::remove_dir_all(root).expect("remove fixture directory");
    }
}
