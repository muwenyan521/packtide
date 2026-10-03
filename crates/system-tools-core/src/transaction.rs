use anyhow::Result;
use std::process::ExitStatus;

use crate::{run_privileged, run_status};

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

pub fn package_upgrade_command(manager: &str) -> PackageUpgradeCommand<'_> {
    match manager {
        "pacman" => PackageUpgradeCommand {
            program: "pacman",
            args: &["-Su"],
            privilege: PackageUpgradePrivilege::Elevated,
        },
        "paru" => PackageUpgradeCommand {
            program: "paru",
            args: &["-Su", "--skipreview"],
            privilege: PackageUpgradePrivilege::User,
        },
        "yay" => PackageUpgradeCommand {
            program: "yay",
            args: &["-Su", "--answeredit", "None"],
            privilege: PackageUpgradePrivilege::User,
        },
        _ => PackageUpgradeCommand {
            program: manager,
            args: &["-Syu"],
            privilege: PackageUpgradePrivilege::User,
        },
    }
}

pub fn run_package_upgrade(manager: &str) -> Result<ExitStatus> {
    let command = package_upgrade_command(manager);
    match command.privilege {
        PackageUpgradePrivilege::Elevated => {
            let args = std::iter::once(command.program)
                .chain(command.args.iter().copied())
                .collect::<Vec<_>>();
            run_privileged(&args)
        }
        PackageUpgradePrivilege::User => run_status(command.program, command.args),
    }
}

/// Builds the upgrade command from a typed backend identifier.
pub fn package_upgrade_command_for(backend: crate::BackendId) -> PackageUpgradeCommand<'static> {
    match backend {
        crate::BackendId::Pacman => PackageUpgradeCommand {
            program: "pacman",
            args: &["-Su"],
            privilege: PackageUpgradePrivilege::Elevated,
        },
        crate::BackendId::Apt => PackageUpgradeCommand {
            program: "apt-get",
            args: &["full-upgrade"],
            privilege: PackageUpgradePrivilege::Elevated,
        },
        crate::BackendId::Paru => PackageUpgradeCommand {
            program: "paru",
            args: &["-Su", "--skipreview"],
            privilege: PackageUpgradePrivilege::User,
        },
        crate::BackendId::Yay => PackageUpgradeCommand {
            program: "yay",
            args: &["-Su", "--answeredit", "None"],
            privilege: PackageUpgradePrivilege::User,
        },
        crate::BackendId::Flatpak => PackageUpgradeCommand {
            program: "flatpak",
            args: &["update"],
            privilege: PackageUpgradePrivilege::User,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{PackageUpgradePrivilege, package_upgrade_command};

    #[test]
    fn package_upgrade_command_uses_privileged_pacman_argv() {
        // Given pacman as the selected package manager
        // When the shared command spec is built
        let command = package_upgrade_command("pacman");

        // Then the invocation is elevated and targets a system upgrade
        assert_eq!(command.program, "pacman");
        assert_eq!(command.args, ["-Su"]);
        assert_eq!(command.privilege, PackageUpgradePrivilege::Elevated);
    }

    #[test]
    fn package_upgrade_command_preserves_paru_review_flags() {
        // Given paru as the selected package manager
        // When the shared command spec is built
        let command = package_upgrade_command("paru");

        // Then paru remains a direct command with review disabled
        assert_eq!(command.program, "paru");
        assert_eq!(command.args, ["-Su", "--skipreview"]);
        assert_eq!(command.privilege, PackageUpgradePrivilege::User);
    }

    #[test]
    fn package_upgrade_command_preserves_yay_edit_answer() {
        // Given yay as the selected package manager
        // When the shared command spec is built
        let command = package_upgrade_command("yay");

        // Then yay remains a direct command with edit prompts disabled
        assert_eq!(command.program, "yay");
        assert_eq!(command.args, ["-Su", "--answeredit", "None"]);
        assert_eq!(command.privilege, PackageUpgradePrivilege::User);
    }

    #[test]
    fn package_upgrade_command_preserves_other_manager_fallback() {
        // Given another manager name
        // When the shared command spec is built
        let command = package_upgrade_command("custom-manager");

        // Then systide's full-upgrade fallback remains direct
        assert_eq!(command.program, "custom-manager");
        assert_eq!(command.args, ["-Syu"]);
        assert_eq!(command.privilege, PackageUpgradePrivilege::User);
    }
}
