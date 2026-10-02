use std::process::ExitStatus;

use crate::{BackendError, BackendId, CapabilitySet, run_privileged, run_status};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
        backend => {
            return Err(BackendError::UnsupportedCapability {
                backend,
                capability: CapabilitySet::SYSTEM_UPGRADE,
            });
        }
    };
    Ok(command)
}

pub fn run_package_upgrade(backend: BackendId) -> Result<ExitStatus, BackendError> {
    let command = package_upgrade_command(backend)?;
    match command.privilege {
        PackageUpgradePrivilege::Elevated => {
            let args = std::iter::once(command.program)
                .chain(command.args.iter().copied())
                .collect::<Vec<_>>();
            run_privileged(&args).map_err(|error| BackendError::CommandFailed {
                backend,
                operation: "upgrade packages",
                message: error.to_string(),
            })
        }
        PackageUpgradePrivilege::User => {
            run_status(command.program, command.args).map_err(|error| BackendError::CommandFailed {
                backend,
                operation: "upgrade packages",
                message: error.to_string(),
            })
        }
    }
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
        run_package_upgrade,
    };
    use crate::{BackendError, BackendId, CapabilitySet};

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
    fn typed_upgrade_command_rejects_unimplemented_backends_without_sentinel() {
        let unsupported = [
            BackendId::Apt,
            BackendId::Dnf5,
            BackendId::Dnf4,
            BackendId::Zypper,
            BackendId::Apk,
            BackendId::Xbps,
            BackendId::Snap,
            BackendId::Brew,
            BackendId::Nix,
        ];

        for backend in unsupported {
            assert_eq!(
                package_upgrade_command_for(backend),
                Err(BackendError::UnsupportedCapability {
                    backend,
                    capability: CapabilitySet::SYSTEM_UPGRADE,
                })
            );
        }
    }

    #[test]
    fn typed_upgrade_rejects_unsupported_backend_before_spawn() {
        let error = run_package_upgrade(BackendId::Apt)
            .expect_err("APT system upgrade is not implemented by this runner");
        assert_eq!(
            error,
            BackendError::UnsupportedCapability {
                backend: BackendId::Apt,
                capability: CapabilitySet::SYSTEM_UPGRADE,
            }
        );
    }
}
