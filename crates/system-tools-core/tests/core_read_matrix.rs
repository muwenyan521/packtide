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
printf 'cmd=%s\n' "${0##*/}" >> "$CORE_READ_MATRIX_LOG"
for arg in "$@"; do printf 'arg=%s\n' "$arg" >> "$CORE_READ_MATRIX_LOG"; done
printf 'end\n' >> "$CORE_READ_MATRIX_LOG"
case "${0##*/}" in
  *) if [ "${CORE_READ_MATRIX_FAILURE-}" = 1 ]; then printf '%s stderr\n' "${0##*/}" >&2; exit 23; fi ;;
esac
case "${0##*/}" in
  pacman|paru|yay) case "$*" in *-Qm*) printf 'hello-aur 1.0\n' ;; *-Q*) printf 'bash 5.2\n' ;; *-Qua*) printf 'bash 5.3\n' ;; *) printf 'core/bash 5.2\n' ;; esac ;;
  checkupdates) printf 'bash 5.3\n' ;;
  apt-cache) cat "$CORE_READ_FIXTURES/apt/catalog.deb822" ;;
  apt-get) cat "$CORE_READ_FIXTURES/apt/updates.txt" ;;
  dpkg-query) cat "$CORE_READ_FIXTURES/apt/installed.tsv" ;;
  dnf5) case "$*" in *--installed*) printf 'bash\t0\t5.2\t1\tx86_64\tfedora\t1\n' ;; *) printf 'bash\t0\t5.2\t1\tx86_64\tfedora\t0\n' ;; esac ;;
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
    for (program, args, count) in expected_read_argv() {
        let mut record = format!("cmd={program}\n");
        for arg in args {
            record.push_str(&format!("arg={arg}\n"));
        }
        record.push_str("end\n");
        assert_eq!(
            calls.matches(&record).count(),
            count,
            "exact argv boundaries: {record:?}"
        );
        println!("exact argv count={count}: {record:?}");
    }
}

fn expected_read_argv() -> Vec<(&'static str, Vec<&'static str>, usize)> {
    vec![
        ("pacman", vec!["--color=never", "-Sl"], 2),
        ("pacman", vec!["--color=never", "-Q"], 1),
        ("pacman", vec!["--color=never", "-Qm"], 2),
        ("pacman", vec!["--color=always", "-Si", "bash"], 1),
        ("checkupdates", vec![], 1),
        ("paru", vec!["-Sl"], 2),
        ("paru", vec!["--color=always", "-Si", "hello-aur"], 1),
        ("paru", vec!["-Qua"], 1),
        ("yay", vec!["-Sl"], 2),
        ("yay", vec!["--color=always", "-Si", "hello-aur"], 1),
        ("yay", vec!["-Qua"], 1),
        ("apt-cache", vec!["dumpavail"], 2),
        ("apt-cache", vec!["show", "--no-all-versions", "bash"], 1),
        (
            "dpkg-query",
            vec![
                "-W",
                "--showformat=${Package}\\t${Version}\\t${Architecture}\\t${Status}\\n",
            ],
            1,
        ),
        ("apt-get", vec!["--just-print", "--simulate", "upgrade"], 1),
        (
            "dnf5",
            vec![
                "repoquery",
                "--qf",
                "%{name}\t%{epoch}\t%{version}\t%{release}\t%{arch}\t%{repoid}\t0\\n",
            ],
            2,
        ),
        (
            "dnf5",
            vec![
                "repoquery",
                "--installed",
                "--qf",
                "%{name}\t%{epoch}\t%{version}\t%{release}\t%{arch}\t%{repoid}\t1\\n",
            ],
            1,
        ),
        ("dnf5", vec!["repoquery", "--info", "bash"], 1),
        (
            "dnf5",
            vec![
                "repoquery",
                "--upgrades",
                "--qf",
                "%{name}\t%{epoch}\t%{version}\t%{release}\t%{arch}\t%{repoid}\t0\\n",
            ],
            1,
        ),
        (
            "dnf",
            vec![
                "repoquery",
                "--qf",
                "%{name}\t%{epoch}\t%{version}\t%{release}\t%{arch}\t%{repoid}\t0\\n",
            ],
            2,
        ),
        (
            "dnf",
            vec![
                "repoquery",
                "--installed",
                "--qf",
                "%{name}\t%{epoch}\t%{version}\t%{release}\t%{arch}\t%{repoid}\t1\\n",
            ],
            1,
        ),
        (
            "dnf",
            vec![
                "repoquery",
                "--upgrades",
                "--qf",
                "%{name}\t%{epoch}\t%{version}\t%{release}\t%{arch}\t%{repoid}\t0\\n",
            ],
            1,
        ),
        ("dnf", vec!["repoquery", "--info", "bash"], 1),
        (
            "zypper",
            vec!["--xmlout", "search", "-s", "-t", "package"],
            1,
        ),
        (
            "zypper",
            vec!["--xmlout", "search", "-s", "-t", "package", "bash"],
            1,
        ),
        (
            "zypper",
            vec!["--xmlout", "search", "-s", "-i", "-t", "package"],
            1,
        ),
        ("zypper", vec!["info", "bash"], 1),
        ("zypper", vec!["--xmlout", "list-updates"], 1),
        ("apk", vec!["search", "--no-cache", "*"], 1),
        ("apk", vec!["search", "--no-cache", "bash"], 1),
        ("apk", vec!["info", "--installed"], 1),
        ("apk", vec!["info", "bash"], 1),
        ("apk", vec!["list", "--upgradable"], 1),
        ("xbps-query", vec!["-Rs", "."], 1),
        ("xbps-query", vec!["-Rs", "bash"], 1),
        ("xbps-query", vec!["-l"], 1),
        ("xbps-query", vec!["-S", "bash"], 1),
        ("xbps-install", vec!["-u", "-n"], 1),
        (
            "flatpak",
            vec![
                "remote-ls",
                "--app",
                "--cached",
                "--columns=application,origin,name",
            ],
            1,
        ),
        (
            "flatpak",
            vec!["list", "--app", "--columns=application,origin,name"],
            2,
        ),
        ("flatpak", vec!["info", "org.example.Hello"], 1),
        (
            "flatpak",
            vec!["remote-ls", "--updates", "--columns=application,version"],
            1,
        ),
        ("snap", vec!["find", "bash"], 1),
        ("snap", vec!["list"], 1),
        ("snap", vec!["info", "hello-world"], 1),
        ("snap", vec!["refresh", "--list"], 1),
        ("brew", vec!["formulae"], 1),
        ("brew", vec!["search", "--formula", "--", "bash"], 1),
        (
            "brew",
            vec!["info", "--json=v2", "--installed", "--formula"],
            1,
        ),
        ("brew", vec!["info", "--json=v2", "--formula", "hello"], 1),
        ("brew", vec!["outdated", "--json=v2"], 1),
        (
            "nix",
            vec![
                "--extra-experimental-features",
                "nix-command flakes",
                "search",
                "--json",
                "nixpkgs",
                "bash",
            ],
            1,
        ),
        (
            "nix",
            vec![
                "--extra-experimental-features",
                "nix-command flakes",
                "profile",
                "list",
                "--json",
            ],
            3,
        ),
        (
            "nix",
            vec![
                "--extra-experimental-features",
                "nix-command flakes",
                "eval",
                "--json",
                "--refresh",
                "github:NixOS/nixpkgs/locked#hello.outPath",
            ],
            1,
        ),
    ]
}

#[test]
fn core_read_matrix_propagates_backend_stderr_for_every_backend() {
    if std::env::var_os("CORE_READ_MATRIX_FAILURE_CHILD").is_some() {
        for backend in BackendId::ALL {
            let error = BuiltinBackend::new(backend)
                .search("failure")
                .expect_err("failed read must not be masked");
            assert!(
                matches!(error, BackendError::CommandFailed { ref message, .. } if message.contains("stderr")),
                "{backend:?} error did not preserve stderr: {error}"
            );
        }
        return;
    }

    let fixture = Fixture::new();
    let script = r##"#!/bin/sh
if [ "${CORE_READ_MATRIX_FAILURE-}" = 1 ]; then
  printf '%s stderr\n' "${0##*/}" >&2
  exit 23
fi
exit 0
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
    let output = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("core_read_matrix_propagates_backend_stderr_for_every_backend")
        .arg("--nocapture")
        .env("CORE_READ_MATRIX_FAILURE_CHILD", "1")
        .env("CORE_READ_MATRIX_FAILURE", "1")
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
}

fn expected_write_args(backend: BackendId, operation: &WriteOperation) -> Vec<&'static str> {
    let (install, remove, upgrade) = match backend {
        BackendId::Pacman | BackendId::Paru | BackendId::Yay => {
            (vec!["-S", "PKG"], vec!["-Rns", "PKG"], vec!["-Su"])
        }
        BackendId::Apt => (
            vec!["install", "PKG"],
            vec!["remove", "PKG"],
            vec!["full-upgrade"],
        ),
        BackendId::Dnf4 | BackendId::Dnf5 => (
            vec!["install", "PKG"],
            vec!["remove", "PKG"],
            vec!["upgrade", "--refresh"],
        ),
        BackendId::Zypper => (
            vec!["install", "PKG"],
            vec!["remove", "PKG"],
            vec!["update"],
        ),
        BackendId::Apk => (vec!["add", "PKG"], vec!["del", "PKG"], vec!["upgrade"]),
        BackendId::Xbps => (vec!["PKG"], vec!["-y", "PKG"], vec!["-Su"]),
        BackendId::Flatpak => (
            vec!["install", "PKG"],
            vec!["uninstall", "PKG"],
            vec!["update"],
        ),
        BackendId::Snap => (
            vec!["install", "PKG"],
            vec!["remove", "PKG"],
            vec!["refresh"],
        ),
        BackendId::Brew => (
            vec!["install", "PKG"],
            vec!["uninstall", "PKG"],
            vec!["upgrade"],
        ),
        BackendId::Nix => (
            vec!["profile", "install", "PKG"],
            vec!["profile", "remove", "PKG"],
            vec!["profile", "upgrade"],
        ),
    };
    match operation {
        WriteOperation::Install { .. } => install,
        WriteOperation::Remove { .. } => remove,
        WriteOperation::SystemUpgrade => upgrade,
        WriteOperation::Upgrade { .. } | WriteOperation::Downgrade { .. } => upgrade,
    }
}

fn assert_exact_write_plan(
    backend: BackendId,
    plan: &system_tools_core::TransactionPlan,
    operation: &WriteOperation,
    key: &str,
) {
    let expected: Vec<String> = if backend == BackendId::Nix {
        let mut v = vec![
            "--extra-experimental-features".into(),
            "nix-command flakes".into(),
        ];
        v.extend(
            expected_write_args(backend, operation)
                .into_iter()
                .map(|arg| {
                    if arg == "PKG" {
                        key.to_string()
                    } else {
                        arg.to_string()
                    }
                }),
        );
        v
    } else if backend == BackendId::Snap && matches!(operation, WriteOperation::Install { .. }) {
        let (name, channel) = key.split_once('@').unwrap();
        vec![
            "install".into(),
            name.into(),
            format!("--channel={}", channel.trim_end_matches("#strict")),
        ]
    } else if backend == BackendId::Snap && matches!(operation, WriteOperation::Remove { .. }) {
        vec!["remove".into(), key.split('@').next().unwrap().into()]
    } else {
        let mut args = expected_write_args(backend, operation)
            .into_iter()
            .map(|arg| {
                if arg == "PKG" {
                    key.to_string()
                } else {
                    arg.to_string()
                }
            })
            .collect::<Vec<_>>();
        if matches!(operation, WriteOperation::Upgrade { .. })
            && matches!(backend, BackendId::Flatpak | BackendId::Brew)
        {
            args.push(key.to_string());
        }
        args
    };
    let actual: Vec<_> = plan
        .command
        .args
        .iter()
        .map(|arg| arg.to_str().unwrap())
        .collect();
    assert_eq!(actual, expected, "{backend:?} {operation:?} argv");
    assert_eq!(
        plan.command.locale.as_deref(),
        Some(std::ffi::OsStr::new("C"))
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
                packages: vec![identity.clone()],
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
        assert_exact_write_plan(
            backend,
            &plan,
            &WriteOperation::Install {
                packages: vec![identity.clone()],
            },
            key,
        );
        let mut write_operations = vec![WriteOperation::Remove {
            packages: vec![identity.clone()],
        }];
        if matches!(backend, BackendId::Flatpak | BackendId::Brew) {
            write_operations.push(WriteOperation::Upgrade {
                packages: vec![identity.clone()],
            });
        }
        if !matches!(
            backend,
            BackendId::Flatpak | BackendId::Brew | BackendId::Nix
        ) {
            write_operations.push(WriteOperation::SystemUpgrade);
        }
        for operation in write_operations {
            let write_plan = b
                .write(operation.clone())
                .unwrap_or_else(|e| panic!("{backend:?} {operation:?}: {e}"));
            assert_exact_write_plan(backend, &write_plan, &operation, key);
            assert_eq!(
                write_plan.command.privilege,
                if scope == PackageScope::System {
                    CommandPrivilege::Elevated
                } else {
                    CommandPrivilege::User
                }
            );
        }
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
