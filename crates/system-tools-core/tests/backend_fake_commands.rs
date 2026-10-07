use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use system_tools_core::{
    BackendId, BuiltinBackend, CapabilitySet, CommandPlan, CommandPrivilege, ExecutableResolver,
    NativePackageKey, PackageBackend, PackageIdentity, PackageKind, PackageScope, WriteOperation,
    run_capture_path, run_command_plan,
};

struct Fixture(PathBuf);

impl Fixture {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "system-tools-core-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir(&path).expect("create fake command directory");
        Self(path)
    }

    fn command(&self, name: &str, body: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write fake command");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("make fake command executable");
        path
    }

    fn resolver(&self) -> ExecutableResolver {
        ExecutableResolver::from_path(Some(self.0.as_os_str()))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn identity(backend: BackendId) -> PackageIdentity {
    PackageIdentity::new(
        backend,
        backend.default_kind(),
        backend.default_scope(),
        NativePackageKey::new("fixture-package").expect("non-empty package key"),
    )
}

fn executable_name(backend: BackendId) -> &'static str {
    match backend {
        BackendId::Apt => "apt-get",
        BackendId::Dnf4 => "dnf",
        BackendId::Xbps => "xbps-install",
        other => other.as_str(),
    }
}

#[test]
fn every_backend_exposes_typed_install_remove_and_upgrade_plans() {
    let fixture = Fixture::new("backend-plan-matrix");
    for name in [
        "pacman",
        "apt-get",
        "dnf5",
        "dnf",
        "zypper",
        "apk",
        "xbps-query",
        "xbps-install",
        "xbps-remove",
        "paru",
        "yay",
        "flatpak",
        "snap",
        "brew",
        "nix",
    ] {
        fixture.command(name, "exit 0");
    }
    let resolver = fixture.resolver();

    for backend in BackendId::ALL {
        let provider = BuiltinBackend::new(backend);
        let package = identity(backend);
        let install = provider
            .write_with_resolver(
                WriteOperation::Install {
                    packages: vec![package.clone()],
                },
                &resolver,
            )
            .unwrap_or_else(|error| panic!("{backend:?} install plan: {error}"));
        let remove = provider
            .write_with_resolver(
                WriteOperation::Remove {
                    packages: vec![package],
                },
                &resolver,
            )
            .unwrap_or_else(|error| panic!("{backend:?} remove plan: {error}"));

        assert_eq!(install.backend, backend);
        assert_eq!(remove.backend, backend);
        assert_eq!(
            install.command.program,
            fixture.0.join(executable_name(backend))
        );
        let remove_program = if backend == BackendId::Xbps {
            "xbps-remove"
        } else {
            executable_name(backend)
        };
        assert_eq!(remove.command.program, fixture.0.join(remove_program));
        assert_eq!(install.command.locale, Some(OsString::from("C")));
        assert_eq!(remove.command.locale, Some(OsString::from("C")));
        assert!(!install.command.args.is_empty(), "{backend:?} install argv");
        assert!(remove.command.args.len() >= 2, "{backend:?} remove argv");

        if provider
            .capabilities()
            .contains(CapabilitySet::SYSTEM_UPGRADE)
        {
            let upgrade = provider
                .write_with_resolver(WriteOperation::SystemUpgrade, &resolver)
                .unwrap_or_else(|error| panic!("{backend:?} upgrade plan: {error}"));
            assert_eq!(upgrade.backend, backend);
            assert_eq!(
                upgrade.command.program,
                fixture.0.join(executable_name(backend))
            );
            assert_eq!(upgrade.command.locale, Some(OsString::from("C")));
            let expected_privilege = if matches!(backend, BackendId::Paru | BackendId::Yay) {
                CommandPrivilege::User
            } else {
                CommandPrivilege::Elevated
            };
            assert_eq!(upgrade.command.privilege, expected_privilege);
        }
    }
}

#[test]
fn write_plans_reject_cross_backend_identity_before_spawning() {
    let fixture = Fixture::new("identity-mismatch");
    fixture.command("apt-get", "exit 0");
    let resolver = fixture.resolver();
    let apt = BuiltinBackend::new(BackendId::Apt);
    let package = PackageIdentity::new(
        BackendId::Brew,
        PackageKind::BrewFormula,
        PackageScope::Profile,
        NativePackageKey::new("display-label").unwrap(),
    );
    let error = apt
        .write_with_resolver(
            WriteOperation::Install {
                packages: vec![package],
            },
            &resolver,
        )
        .expect_err("identity must be validated before command lookup");
    assert!(matches!(
        error,
        system_tools_core::BackendError::IdentityMismatch {
            expected_backend: BackendId::Apt,
            actual_backend: BackendId::Brew,
            ..
        }
    ));
}

#[test]
fn fake_command_captures_locale_argv_stdout_stderr_and_failure_status() {
    let fixture = Fixture::new("failure-contract");
    let script = fixture.command(
        "fake-backend",
        "printf '%s\\n' \"$*\" > \"$FAKE_ARGV\"\nprintf '%s\\n' \"$LC_ALL\" > \"$FAKE_LOCALE\"\nprintf 'fixture stdout\\n'\nprintf 'fixture stderr\\n' >&2\nexit 23",
    );
    let argv = fixture.0.join("argv");
    let locale = fixture.0.join("locale");
    let plan = CommandPlan::new(script.clone())
        .with_locale("C")
        .with_privilege(CommandPrivilege::User)
        .arg("--machine-readable")
        .arg("fixture-key");
    let error = run_command_plan(&plan).expect_err("failed status must propagate");
    assert!(error.to_string().contains("exited"));
    let command = std::process::Command::new(&script)
        .args(&plan.args)
        .env("FAKE_ARGV", &argv)
        .env("FAKE_LOCALE", &locale)
        .env("LC_ALL", "C")
        .output()
        .expect("execute fake command with explicit environment");
    assert_eq!(command.status.code(), Some(23));
    assert_eq!(
        fs::read_to_string(argv).unwrap(),
        "--machine-readable fixture-key\n"
    );
    assert_eq!(fs::read_to_string(locale).unwrap(), "C\n");

    let direct = run_capture_path(&script, &["--failure"], true).expect("capture failed status");
    assert_eq!(direct.status.code(), Some(23));
    assert!(direct.stderr.contains("fixture stderr"));
}
