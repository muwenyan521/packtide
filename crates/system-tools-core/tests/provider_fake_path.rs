use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use system_tools_core::{
    BackendError, BackendId, BuiltinBackend, CommandPrivilege, NativePackageKey, PackageBackend,
    PackageIdentity, PackageKind, PackageScope, WriteOperation, run_command_plan,
};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "system-tools-core-provider-fake-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create isolated fake PATH");
        Self(path)
    }

    fn write_executable(&self, name: &str, body: &str) {
        let path = self.0.join(name);
        fs::write(&path, body).expect("write fake provider");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("make fake provider executable");
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!("cannot remove fake PATH {}: {error}", self.0.display());
        }
    }
}

fn package(
    backend: BackendId,
    kind: PackageKind,
    scope: PackageScope,
    key: &str,
) -> PackageIdentity {
    PackageIdentity::new(
        backend,
        kind,
        scope,
        NativePackageKey::new(key).expect("fixture package key is non-empty"),
    )
}

fn invoke_fixture(
    test_executable: &Path,
    fake_path: &Path,
    mode: &str,
    log: &Path,
) -> std::process::Output {
    Command::new(test_executable)
        .args([
            "--exact",
            "fake_path_provider_read_write_contracts",
            "--nocapture",
        ])
        .env("PROVIDER_FAKE_MODE", mode)
        .env("PROVIDER_FAKE_LOG", log)
        .env(
            "PATH",
            std::env::join_paths([fake_path, Path::new("/usr/bin"), Path::new("/bin")])
                .expect("build fake PATH"),
        )
        .output()
        .expect("run provider fake-path child")
}

#[test]
fn fake_path_provider_read_write_contracts() {
    if let Ok(mode) = std::env::var("PROVIDER_FAKE_MODE") {
        run_child_contract(&mode);
        return;
    }

    let fixture = Fixture::new();
    let script = r##"#!/bin/sh
printf 'uid=%s lc_all=%s' "$(id -u)" "${LC_ALL-}" >> "$PROVIDER_FAKE_LOG"
for arg in "$@"; do printf '\t%s' "$arg" >> "$PROVIDER_FAKE_LOG"; done
printf '\n' >> "$PROVIDER_FAKE_LOG"
case "$PROVIDER_FAKE_MODE:$1" in
  snap-search:find) printf 'Name Version Publisher Notes Summary\nhello 1.0 Acme - Hello\n' ;;
  snap-failure:find) printf 'snap fake failure\n' >&2; exit 19 ;;
  snap-install:install) exit 0 ;;
  brew-catalog:formulae) printf 'hello\n' ;;
  brew-install:install) exit 0 ;;
  nix-installed:*) printf '%s\n' '{"elements":{"hello":{"attrPath":"legacyPackages.x86_64-linux.hello","originalUrl":"flake:nixpkgs","storePaths":["/nix/store/old-hello"],"active":true}}}' ;;
  nix-install:*) exit 0 ;;
  *) printf 'unexpected fake provider invocation\n' >&2; exit 64 ;;
esac
"##;
    for provider in ["snap", "brew", "nix"] {
        fixture.write_executable(provider, script);
    }

    for mode in [
        "snap-search",
        "snap-failure",
        "brew-catalog",
        "brew-install",
        "nix-installed",
        "nix-install",
    ] {
        let log = fixture.path().join(format!("{mode}.log"));
        let output = invoke_fixture(
            &std::env::current_exe().expect("locate test executable"),
            fixture.path(),
            mode,
            &log,
        );
        assert!(
            output.status.success(),
            "mode {mode} failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(log.exists(), "mode {mode} did not invoke a fake provider");
    }

    assert_eq!(
        fs::read_to_string(fixture.path().join("snap-search.log")).unwrap(),
        format!("uid={} lc_all=C\tfind\thello\n", userspace_uid()),
    );
    assert_eq!(
        fs::read_to_string(fixture.path().join("brew-catalog.log")).unwrap(),
        format!("uid={} lc_all=C\tformulae\n", userspace_uid()),
    );
    assert_eq!(
        fs::read_to_string(fixture.path().join("brew-install.log")).unwrap(),
        format!("uid={} lc_all=C\tinstall\thello\n", userspace_uid()),
    );
    assert_eq!(
        fs::read_to_string(fixture.path().join("nix-installed.log")).unwrap(),
        format!(
            "uid={} lc_all=C\t--extra-experimental-features\tnix-command flakes\tprofile\tlist\t--json\n",
            userspace_uid()
        ),
    );
    assert_eq!(
        fs::read_to_string(fixture.path().join("nix-install.log")).unwrap(),
        format!(
            "uid={} lc_all=C\t--extra-experimental-features\tnix-command flakes\tprofile\tinstall\tnixpkgs#hello\n",
            userspace_uid()
        ),
    );
}

fn userspace_uid() -> u32 {
    let output = Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .expect("read current uid");
    String::from_utf8(output.stdout)
        .expect("uid is utf8")
        .trim()
        .parse()
        .expect("uid is numeric")
}

fn run_child_contract(mode: &str) {
    let log = PathBuf::from(std::env::var("PROVIDER_FAKE_LOG").expect("fake log path"));
    match mode {
        "snap-search" => {
            let result = BuiltinBackend::new(BackendId::Snap)
                .search("hello")
                .expect("fake snap search succeeds");
            assert_eq!(
                result.packages[0].native_key.as_str(),
                "hello@latest/stable#strict"
            );
        }
        "snap-failure" => {
            let error = BuiltinBackend::new(BackendId::Snap)
                .search("hello")
                .expect_err("fake snap failure is propagated");
            assert!(matches!(
                error,
                BackendError::CommandFailed { message, .. }
                    if message.contains("snap fake failure")
            ));
        }
        "brew-catalog" => {
            let result = BuiltinBackend::new(BackendId::Brew)
                .catalog()
                .expect("fake brew catalog succeeds");
            assert_eq!(result.packages[0].native_key.as_str(), "hello");
        }
        "brew-install" => {
            let backend = BuiltinBackend::new(BackendId::Brew);
            let identity = package(
                BackendId::Brew,
                PackageKind::BrewFormula,
                PackageScope::Profile,
                "hello",
            );
            let plan = backend
                .write(WriteOperation::Install {
                    packages: vec![identity],
                })
                .expect("fake brew install plan");
            assert_eq!(
                plan.command.locale.as_deref(),
                Some(std::ffi::OsStr::new("C"))
            );
            assert_eq!(plan.command.privilege, CommandPrivilege::User);
            run_command_plan(&plan.command).expect("fake brew install succeeds");
        }
        "nix-installed" => {
            let result = BuiltinBackend::new(BackendId::Nix)
                .installed()
                .expect("fake nix profile list succeeds");
            assert_eq!(result.packages[0].native_key.as_str(), "hello");
        }
        "nix-install" => {
            let backend = BuiltinBackend::new(BackendId::Nix);
            let identity = package(
                BackendId::Nix,
                PackageKind::Nix,
                PackageScope::Profile,
                "nixpkgs#hello",
            );
            let plan = backend
                .write(WriteOperation::Install {
                    packages: vec![identity],
                })
                .expect("fake nix install plan");
            assert_eq!(
                plan.command.locale.as_deref(),
                Some(std::ffi::OsStr::new("C"))
            );
            assert_eq!(plan.command.privilege, CommandPrivilege::User);
            run_command_plan(&plan.command).expect("fake nix install succeeds");
        }
        _ => panic!("unknown fake provider mode {mode}"),
    }
    assert!(log.exists(), "fake provider did not record invocation");
}
