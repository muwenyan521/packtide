use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use system_tools_core::{
    BackendError, BackendId, BuiltinBackend, PackageBackend, PackageId, ReadOperation,
};

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove failure fixtures");
    }
}

fn operation(name: &str, backend: BuiltinBackend) -> ReadOperation {
    match name {
        "catalog" => ReadOperation::Catalog,
        "search" => ReadOperation::Search {
            query: "bash".into(),
        },
        "installed" => ReadOperation::Installed,
        "details" => ReadOperation::Details {
            package: PackageId::new("bash").unwrap(),
            scope: backend.scope(),
        },
        "updates" => ReadOperation::Updates,
        _ => panic!("unknown read operation: {name}"),
    }
}

fn run_child(backend_name: &str) {
    let id = BackendId::ALL
        .into_iter()
        .find(|id| id.as_str() == backend_name)
        .unwrap();
    let backend = BuiltinBackend::new(id);
    let name = std::env::var("READ_FAILURE_OPERATION").unwrap();
    let mode = std::env::var("READ_FAILURE_MODE").unwrap();
    let marker = format!("{backend_name}/{name} controlled stderr");
    let result = backend.read(operation(&name, backend));
    if name == "catalog" && matches!(id, BackendId::Snap | BackendId::Nix) {
        assert!(matches!(result, Err(BackendError::QueryRequired { backend }) if backend == id));
        println!("{backend_name}/{name}/{mode}: QueryRequired; no command");
        return;
    }
    let stderr = if mode == "stderr" {
        marker.as_str()
    } else {
        ""
    };
    if name == "details"
        && matches!(
            id,
            BackendId::Pacman
                | BackendId::Paru
                | BackendId::Yay
                | BackendId::Apt
                | BackendId::Dnf4
                | BackendId::Dnf5
                | BackendId::Flatpak
        )
    {
        let details = result.unwrap().details.expect("failed details payload");
        assert!(!details.success);
        assert_eq!(details.status.code(), Some(23));
        assert_eq!(details.stderr.trim_end(), stderr);
        println!(
            "{backend_name}/{name}/{mode}: details status=23 stderr={:?}",
            details.stderr
        );
    } else {
        let error = result.expect_err("failed read must not be masked");
        assert!(
            matches!(&error, BackendError::CommandFailed { backend, message, .. }
            if *backend == id && message == if mode == "stderr" {
                &marker
            } else {
                "command exited with exit status: 23"
            }),
            "{backend_name}/{name}/{mode}: {error}"
        );
        println!("{backend_name}/{name}/{mode}: {error}");
    }
}

#[test]
fn every_read_operation_preserves_controlled_failure_status_and_stderr() {
    if let Ok(backend) = std::env::var("READ_FAILURE_BACKEND") {
        run_child(&backend);
        return;
    }
    // Given: fresh HOME/cache per scenario and controlled executable failures.
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "system-tools-read-failure-{}-{}", std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    )));
    fs::create_dir_all(&fixture.0).unwrap();
    let script = r#"#!/bin/sh
printf 'cmd=%s status=23 mode=%s lc_all=%s\n' "${0##*/}" "$READ_FAILURE_MODE" "${LC_ALL-}" >> "$READ_FAILURE_LOG"
for arg in "$@"; do printf 'arg=%s\n' "$arg" >> "$READ_FAILURE_LOG"; done
if [ "$READ_FAILURE_MODE" = stderr ]; then
  printf '%s/%s controlled stderr\n' "$READ_FAILURE_BACKEND" "$READ_FAILURE_OPERATION" >&2
fi
exit 23
"#;
    for program in [
        "pacman",
        "checkupdates",
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
    ] {
        let path = fixture.0.join(program);
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    // When: one isolated consumer process executes each provider/operation/failure mode.
    for id in BackendId::ALL {
        for name in ["catalog", "search", "installed", "details", "updates"] {
            for mode in ["stderr", "status"] {
                let home = fixture.0.join(format!("{}-{name}-{mode}", id.as_str()));
                fs::create_dir(&home).unwrap();
                let log = home.join("calls.log");
                let output = Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "every_read_operation_preserves_controlled_failure_status_and_stderr",
                        "--nocapture",
                    ])
                    .env("READ_FAILURE_BACKEND", id.as_str())
                    .env("READ_FAILURE_OPERATION", name)
                    .env("READ_FAILURE_MODE", mode)
                    .env("READ_FAILURE_LOG", &log)
                    .env("HOME", &home)
                    .env("XDG_CACHE_HOME", &home)
                    .env("PATH", &fixture.0)
                    .output()
                    .unwrap();
                // Then: consumer assertions and executable traces prove the public failure contract.
                println!("{}", String::from_utf8_lossy(&output.stdout));
                assert!(
                    output.status.success(),
                    "{} {name}/{mode}: {}",
                    id.as_str(),
                    String::from_utf8_lossy(&output.stderr)
                );
                let no_command =
                    name == "catalog" && matches!(id, BackendId::Snap | BackendId::Nix);
                if no_command {
                    assert!(
                        !log.exists(),
                        "unsupported/query-required read executed a command"
                    );
                } else {
                    let calls = fs::read_to_string(log).unwrap();
                    assert!(calls.contains("status=23"));
                    if id == BackendId::Dnf4
                        || (matches!(id, BackendId::Paru | BackendId::Yay) && name == "installed")
                    {
                        let expected = expected_failure_argv(id, name);
                        let mut lines = calls.lines();
                        let command = lines.next().unwrap();
                        assert!(
                            command.starts_with(&format!("cmd={} ", expected.0)),
                            "unexpected command trace: {command:?}"
                        );
                        let actual = lines
                            .map(|line| line.strip_prefix("arg=").unwrap().to_string())
                            .collect::<Vec<_>>();
                        assert_eq!(actual, expected.1, "{}/{} failure argv", id.as_str(), name);
                    }
                    println!("{calls}");
                }
            }
        }
    }
}

fn expected_failure_argv(backend: BackendId, operation: &str) -> (&'static str, Vec<String>) {
    match (backend, operation) {
        (BackendId::Dnf4, "catalog") | (BackendId::Dnf4, "search") => (
            "dnf",
            vec![
                "repoquery".into(),
                "--qf".into(),
                "%{name}\t%{epoch}\t%{version}\t%{release}\t%{arch}\t%{repoid}\t0".into(),
            ],
        ),
        (BackendId::Dnf4, "installed") => (
            "dnf",
            vec![
                "repoquery".into(),
                "--installed".into(),
                "--qf".into(),
                "%{name}\t%{epoch}\t%{version}\t%{release}\t%{arch}\t%{repoid}\t1".into(),
            ],
        ),
        (BackendId::Dnf4, "details") => (
            "dnf",
            vec!["repoquery".into(), "--info".into(), "bash".into()],
        ),
        (BackendId::Dnf4, "updates") => (
            "dnf",
            vec![
                "repoquery".into(),
                "--upgrades".into(),
                "--qf".into(),
                "%{name}\t%{epoch}\t%{version}\t%{release}\t%{arch}\t%{repoid}\t0".into(),
            ],
        ),
        (BackendId::Paru, "installed") | (BackendId::Yay, "installed") => {
            ("pacman", vec!["--color=never".into(), "-Qm".into()])
        }
        _ => panic!("unsupported focused argv assertion: {backend:?}/{operation}"),
    }
}
