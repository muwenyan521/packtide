use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use system_tools_core::{
    BackendError, BackendId, BuiltinBackend, CommandPrivilege, NativePackageKey, PackageBackend,
    PackageId, PackageIdentity, PackageKind, PackageScope, ReadOperation, WriteOperation,
};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "system-tools-core-read-matrix-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn install(&self, name: &str, body: &str) {
        let path = self.0.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/package-managers/fixtures")
}

fn expected(backend: BackendId) -> (&'static str, PackageKind, PackageScope) {
    match backend {
        BackendId::Pacman
        | BackendId::Apt
        | BackendId::Dnf5
        | BackendId::Dnf4
        | BackendId::Zypper
        | BackendId::Apk
        | BackendId::Xbps => ("bash", PackageKind::System, PackageScope::System),
        BackendId::Paru | BackendId::Yay => ("hello-aur", PackageKind::Aur, PackageScope::User),
        BackendId::Flatpak => (
            "org.example.Hello",
            PackageKind::Flatpak,
            PackageScope::User,
        ),
        BackendId::Snap => (
            "hello-world@6.4/stable#strict",
            PackageKind::Snap,
            PackageScope::System,
        ),
        BackendId::Brew => ("hello", PackageKind::BrewFormula, PackageScope::Profile),
        BackendId::Nix => ("1", PackageKind::Nix, PackageScope::Profile),
    }
}

#[test]
fn core_read_matrix_fake_path_dispatches_every_backend_operation() {
    if std::env::var_os("CORE_READ_MATRIX_CHILD").is_some() {
        run_child();
        return;
    }
    let fixture = Fixture::new();
    let script = r##"#!/bin/sh
printf 'uid=%s lc_all=%s\n' "$(id -u)" "${LC_ALL-}" >> "$CORE_READ_MATRIX_LOG"
for arg in "$@"; do printf 'arg=%s\n' "$arg" >> "$CORE_READ_MATRIX_LOG"; done
case "${0##*/}" in
  pacman|paru|yay) case "$*" in *-Qm*) printf 'hello-aur 1.0\n' ;; *-Q*) printf 'bash 5.2\n' ;; *-Qua*) printf 'bash 5.3\n' ;; *) printf 'core/bash 5.2\n' ;; esac ;;
  checkupdates) printf 'bash 5.3\n' ;;
  apt-cache) cat "$CORE_READ_FIXTURES/apt/catalog.deb822" ;;
  apt-get) cat "$CORE_READ_FIXTURES/apt/updates.txt" ;;
  dpkg-query) cat "$CORE_READ_FIXTURES/apt/installed.tsv" ;;
  dnf5) printf '%s\n' '[{"name":"bash","version":"5.2","arch":"x86_64"}]' ;;
  dnf) printf 'bash\t0\t5.2\t1\tx86_64\tfedora\t1\n' ;;
  zypper) cat "$CORE_READ_FIXTURES/zypper/search.xml" ;;
  apk) cat "$CORE_READ_FIXTURES/apk/search.tsv" ;;
  xbps-query) cat "$CORE_READ_FIXTURES/xbps/search.tsv" ;;
  xbps-install|xbps-remove) cat "$CORE_READ_FIXTURES/xbps/updates.tsv" ;;
  flatpak) printf 'org.example.Hello\tflathub\tHello\tuser\n' ;;
  snap) case "$1:$2" in find:failure) printf 'fixture stderr\n' >&2; exit 23 ;; find:*) cat "$CORE_READ_FIXTURES/snap/find.txt" ;; list:*) cat "$CORE_READ_FIXTURES/snap/list.txt" ;; info:*) cat "$CORE_READ_FIXTURES/snap/info.txt" ;; refresh:*) cat "$CORE_READ_FIXTURES/snap/refresh-list.txt" ;; esac ;;
  brew) case "$1" in info) cat "$CORE_READ_FIXTURES/brew/info.json" ;; outdated) cat "$CORE_READ_FIXTURES/brew/outdated.json" ;; *) printf 'hello\n' ;; esac ;;
  nix) case "$*" in *' search '* ) printf '{"hello":{"pname":"bash"}}\n' ;; *' eval '* ) printf '"/nix/store/new-hello"\n' ;; *) cat "$CORE_READ_FIXTURES/nix/profile.json" ;; esac ;;
  *) printf 'fixture stderr\n' >&2; exit 23 ;;
esac
"##;
    for name in [
        "pacman",
        "paru",
        "yay",
        "apt-cache",
        "apt-get",
        "dpkg-query",
        "dnf5",
        "dnf",
        "zypper",
        "apk",
        "xbps-query",
        "xbps-install",
        "xbps-remove",
        "flatpak",
        "snap",
        "brew",
        "nix",
        "checkupdates",
    ] {
        fixture.install(name, script);
    }
    let log = fixture.0.join("calls.log");
    let output = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("core_read_matrix_fake_path_dispatches_every_backend_operation")
        .arg("--nocapture")
        .env("CORE_READ_MATRIX_CHILD", "1")
        .env("CORE_READ_MATRIX_LOG", &log)
        .env("CORE_READ_FIXTURES", fixture_path())
        .env("HOME", &fixture.0)
        .env("XDG_CACHE_HOME", &fixture.0)
        .env(
            "PATH",
            std::env::join_paths([
                fixture.0.as_path(),
                Path::new("/usr/bin"),
                Path::new("/bin"),
            ])
            .unwrap(),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let calls = fs::read_to_string(log).unwrap();
    assert!(
        calls.lines().any(|line| line.contains("lc_all=C")),
        "fake commands did not receive LC_ALL=C: {calls}"
    );
}

fn run_child() {
    for backend in BackendId::ALL {
        let b = BuiltinBackend::new(backend);
        let (key, kind, scope) = expected(backend);
        let identity =
            PackageIdentity::new(backend, kind, scope, NativePackageKey::new(key).unwrap());
        let plan = b
            .write(WriteOperation::Install {
                packages: vec![identity],
            })
            .unwrap_or_else(|e| panic!("{backend:?} install plan: {e}"));
        assert_eq!(
            plan.command.locale.as_deref(),
            Some(std::ffi::OsStr::new("C"))
        );
        assert_eq!(
            plan.command.privilege,
            if scope == PackageScope::System {
                CommandPrivilege::Elevated
            } else {
                CommandPrivilege::User
            }
        );
        for operation in [
            ReadOperation::Catalog,
            ReadOperation::Search {
                query: "bash".into(),
            },
            ReadOperation::Installed,
            ReadOperation::Details {
                package: PackageId::new(key).unwrap(),
                scope,
            },
            ReadOperation::Updates,
        ] {
            let result = b.read(operation.clone());
            if matches!(backend, BackendId::Snap | BackendId::Nix)
                && matches!(operation, ReadOperation::Catalog)
            {
                assert!(
                    matches!(result, Err(BackendError::QueryRequired { .. })),
                    "{backend:?} catalog"
                );
                continue;
            }
            let result = result.unwrap_or_else(|e| panic!("{backend:?} {operation:?}: {e}"));
            assert_eq!(result.backend, backend);
            if !result.packages.is_empty() {
                assert!(
                    result
                        .packages
                        .iter()
                        .all(|p| p.backend == backend && p.kind == kind && p.scope == scope),
                    "{backend:?} typed identity: {:?}",
                    result.packages
                );
            }
            if matches!(operation, ReadOperation::Details { .. }) {
                let details = result.details.expect("details payload");
                assert!(details.success);
                assert!(details.status.success());
            }
        }
    }
    let error = BuiltinBackend::new(BackendId::Snap)
        .search("failure")
        .expect_err("fake stderr failure is propagated");
    assert!(
        matches!(error, BackendError::CommandFailed { message, .. } if message.contains("fixture stderr"))
    );
}
