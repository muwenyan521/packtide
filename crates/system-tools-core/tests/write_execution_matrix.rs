use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use system_tools_core::{
    BackendId, BuiltinBackend, CapabilitySet, CommandPrivilege, ExecutableResolver,
    NativePackageKey, PackageBackend, PackageIdentity, WriteOperation, run_command_plan,
};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("write-matrix-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).expect("create isolated fake PATH");
        Self(path)
    }

    fn commands(&self, status: i32) {
        for name in [
            "pacman",
            "paru",
            "yay",
            "flatpak",
            "apt-get",
            "dnf5",
            "dnf",
            "zypper",
            "apk",
            "xbps-query",
            "xbps-install",
            "xbps-remove",
            "snap",
            "brew",
            "nix",
        ] {
            let path = self.0.join(name);
            fs::write(
                &path,
                format!(
                    "#!/bin/sh\nprintf 'locale=%s\\n' \"$LC_ALL\"\nfor arg in \"$@\"; do printf 'arg=%s\\n' \"$arg\"; done\nprintf 'write fixture stderr\\n' >&2\nexit {status}\n"
                ),
            )
            .expect("write fake command");
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))
                .expect("chmod fake command");
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!("fixture cleanup failed: {error}");
        }
    }
}

fn expected_args(backend: BackendId, action: &str) -> Vec<OsString> {
    let args: &[&str] = match (backend, action) {
        (BackendId::Pacman | BackendId::Paru | BackendId::Yay, "install") => {
            &["-S", "fixture-package"]
        }
        (BackendId::Pacman | BackendId::Paru | BackendId::Yay, "remove") => {
            &["-Rns", "fixture-package"]
        }
        (BackendId::Pacman | BackendId::Paru | BackendId::Yay | BackendId::Xbps, "upgrade") => {
            &["-Su"]
        }
        (
            BackendId::Apt
            | BackendId::Dnf4
            | BackendId::Dnf5
            | BackendId::Zypper
            | BackendId::Brew
            | BackendId::Snap,
            "install",
        ) => &["install", "fixture-package"],
        (
            BackendId::Apt
            | BackendId::Dnf4
            | BackendId::Dnf5
            | BackendId::Zypper
            | BackendId::Snap,
            "remove",
        ) => &["remove", "fixture-package"],
        (BackendId::Apt, "upgrade") => &["full-upgrade"],
        (BackendId::Dnf4 | BackendId::Dnf5, "upgrade") => &["upgrade", "--refresh"],
        (BackendId::Zypper, "upgrade") => &["update"],
        (BackendId::Apk, "install") => &["add", "fixture-package"],
        (BackendId::Apk, "remove") => &["del", "fixture-package"],
        (BackendId::Apk, "upgrade") => &["upgrade"],
        (BackendId::Xbps, "install") => &["fixture-package"],
        (BackendId::Xbps, "remove") => &["-y", "fixture-package"],
        (BackendId::Flatpak, "install") => &["install", "fixture-package"],
        (BackendId::Flatpak | BackendId::Brew, "remove") => &["uninstall", "fixture-package"],
        (BackendId::Flatpak, "upgrade") => &["update", "fixture-package"],
        (BackendId::Snap, "upgrade") => &["refresh"],
        (BackendId::Brew, "upgrade") => &["upgrade", "fixture-package"],
        (BackendId::Nix, "install") => &[
            "--extra-experimental-features",
            "nix-command flakes",
            "profile",
            "install",
            "fixture-package",
        ],
        (BackendId::Nix, "remove") => &[
            "--extra-experimental-features",
            "nix-command flakes",
            "profile",
            "remove",
            "fixture-package",
        ],
        (BackendId::Nix, "upgrade") => &[
            "--extra-experimental-features",
            "nix-command flakes",
            "profile",
            "upgrade",
            "fixture-package",
        ],
        _ => panic!("unexpected matrix case {backend:?}/{action}"),
    };
    args.iter().map(OsString::from).collect()
}

#[test]
fn every_backend_write_executes_exact_argv_and_exposes_success_or_failure() {
    let fixture = Fixture::new();
    let resolver = ExecutableResolver::from_path(Some(fixture.0.as_os_str()));
    for status in [0, 23] {
        fixture.commands(status);
        for backend in BackendId::ALL {
            let provider = BuiltinBackend::new(backend);
            let package = PackageIdentity::new(
                backend,
                backend.default_kind(),
                backend.default_scope(),
                NativePackageKey::new("fixture-package").expect("fixture key"),
            );
            for action in ["install", "remove", "upgrade"] {
                let operation = match action {
                    "install" => WriteOperation::Install {
                        packages: vec![package.clone()],
                    },
                    "remove" => WriteOperation::Remove {
                        packages: vec![package.clone()],
                    },
                    "upgrade"
                        if provider
                            .capabilities()
                            .contains(CapabilitySet::SYSTEM_UPGRADE) =>
                    {
                        WriteOperation::SystemUpgrade
                    }
                    "upgrade" => WriteOperation::Upgrade {
                        packages: vec![package.clone()],
                    },
                    _ => unreachable!(),
                };
                let plan = provider
                    .write_with_resolver(operation, &resolver)
                    .unwrap_or_else(|error| panic!("{backend:?}/{action}: {error}"));
                let expected_privilege = match backend {
                    BackendId::Paru
                    | BackendId::Yay
                    | BackendId::Flatpak
                    | BackendId::Brew
                    | BackendId::Nix => CommandPrivilege::User,
                    BackendId::Pacman
                    | BackendId::Apt
                    | BackendId::Dnf4
                    | BackendId::Dnf5
                    | BackendId::Zypper
                    | BackendId::Apk
                    | BackendId::Xbps
                    | BackendId::Snap => CommandPrivilege::Elevated,
                };
                assert_eq!(plan.command.privilege, expected_privilege);
                assert_eq!(plan.command.locale.as_deref(), Some(OsStr::new("C")));
                let args = expected_args(backend, action);
                assert_eq!(plan.command.args, args, "{backend:?}/{action}");
                let program = match (backend, action) {
                    (BackendId::Apt, _) => "apt-get",
                    (BackendId::Dnf4, _) => "dnf",
                    (BackendId::Xbps, "remove") => "xbps-remove",
                    (BackendId::Xbps, _) => "xbps-install",
                    _ => backend.as_str(),
                };
                assert_eq!(plan.command.program, fixture.0.join(program));

                let mut command = Command::new(&plan.command.program);
                command.args(&plan.command.args).env("PATH", &fixture.0);
                for key in &plan.command.env_remove {
                    command.env_remove(key);
                }
                command.env("LC_ALL", plan.command.locale.as_ref().expect("locale"));
                let output = command.output().expect("execute fake provider");

                assert_eq!(output.status.code(), Some(status));
                let expected_stdout = format!(
                    "locale=C\n{}",
                    args.iter()
                        .map(|arg| format!("arg={}\n", arg.to_string_lossy()))
                        .collect::<String>()
                );
                assert_eq!(String::from_utf8_lossy(&output.stdout), expected_stdout);
                assert_eq!(output.stderr, b"write fixture stderr\n");
                if expected_privilege == CommandPrivilege::User {
                    let result = run_command_plan(&plan.command);
                    if status == 0 {
                        assert!(result.expect("production user runner succeeds").success());
                    } else {
                        assert!(
                            result
                                .expect_err("production user runner propagates failure")
                                .to_string()
                                .contains("23")
                        );
                    }
                }
                println!(
                    "{backend:?}/{action}: privilege={expected_privilege:?} status={status} stderr={} stdout={expected_stdout:?}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
        }
    }
}
