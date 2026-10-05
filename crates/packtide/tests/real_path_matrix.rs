use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("packtide-real-path-{nonce}"));
        fs::create_dir(&path).expect("create fixture");
        Self(path)
    }

    fn command(&self, name: &str, body: &str) {
        let path = self.0.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write provider");
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod provider");
    }

    fn path(&self) -> std::ffi::OsString {
        std::env::join_paths([self.0.as_path(), Path::new("/usr/bin"), Path::new("/bin")])
            .expect("build PATH")
    }

    fn run_preview(&self, row: &str) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_packtide"))
            .args(["__preview", "remove", row])
            .env("PATH", self.path())
            .env("PACKTIDE_UI_LANG", "en")
            .output()
            .expect("run preview")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn preview_matrix_resolves_hidden_identity_and_exact_provider_argv() {
    let cases = [
        (
            "apt-cache",
            "APT:apt|b=apt|s=system|k=system|n=68656c6c6f",
            "show --no-all-versions hello",
        ),
        (
            "dnf5",
            "DNF:dnf|b=dnf5|s=system|k=system|n=68656c6c6f",
            "info hello",
        ),
        (
            "dnf",
            "DNF:dnf|b=dnf4|s=system|k=system|n=68656c6c6f",
            "repoquery --info hello",
        ),
        (
            "zypper",
            "ZYPPER:zypper|b=zypper|s=system|k=system|n=68656c6c6f",
            "--xmlout info hello",
        ),
        (
            "apk",
            "APK:apk|b=apk|s=system|k=system|n=68656c6c6f",
            "info hello",
        ),
        (
            "xbps-query",
            "XBPS:xbps|b=xbps|s=system|k=system|n=68656c6c6f",
            "-S hello",
        ),
        (
            "snap",
            "SNAP:snap|b=snap|s=system|k=snap|n=68656c6c6f",
            "info hello",
        ),
        (
            "brew",
            "BREW:brew|b=brew|s=profile|k=brew-formula|n=68656c6c6f",
            "info --json=v2 --formula hello",
        ),
        (
            "nix",
            "NIX:nix|b=nix|s=profile|k=nix|n=68656c6c6f",
            "--extra-experimental-features nix-command flakes profile list --json",
        ),
    ];

    for (program, row, expected) in cases {
        let fixture = Fixture::new();
        let log = fixture.0.join("argv");
        fixture.command(
            program,
            &format!(
                "printf '%s\\n' \"$*\" > '{}'\nprintf 'name: hello\\nsummary: fixture\\n'",
                log.display()
            ),
        );
        if program == "apt-cache" {
            fixture.command("apt-get", "exit 0");
            fixture.command("dpkg-query", "exit 0");
        }
        if program == "xbps-query" {
            fixture.command("xbps-install", "exit 0");
            fixture.command("xbps-remove", "exit 0");
        }
        let output = fixture.run_preview(&format!("{row}\tSource\tShown name\t1"));
        assert!(
            output.status.success(),
            "{program}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let argv = fs::read_to_string(&log).expect("provider argv log");
        assert_eq!(
            argv,
            format!("{expected}\n"),
            "{program}: wrong provider argv"
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Shown"),
            "{program}: preview omitted display name: {stdout}"
        );
    }
}

#[test]
fn preview_matrix_keeps_process_success_and_reports_provider_failure() {
    let cases = [
        ("apt-cache", "APT:apt|b=apt|s=system|k=system|n=68656c6c6f"),
        ("dnf5", "DNF:dnf|b=dnf5|s=system|k=system|n=68656c6c6f"),
        ("dnf", "DNF:dnf|b=dnf4|s=system|k=system|n=68656c6c6f"),
        (
            "zypper",
            "ZYPPER:zypper|b=zypper|s=system|k=system|n=68656c6c6f",
        ),
        ("apk", "APK:apk|b=apk|s=system|k=system|n=68656c6c6f"),
        (
            "xbps-query",
            "XBPS:xbps|b=xbps|s=system|k=system|n=68656c6c6f",
        ),
        ("snap", "SNAP:snap|b=snap|s=system|k=snap|n=68656c6c6f"),
        (
            "brew",
            "BREW:brew|b=brew|s=profile|k=brew-formula|n=68656c6c6f",
        ),
        ("nix", "NIX:nix|b=nix|s=profile|k=nix|n=68656c6c6f"),
    ];
    for (program, row) in cases {
        let fixture = Fixture::new();
        fixture.command(program, "printf 'provider failed\\n' >&2; exit 23");
        if program == "apt-cache" {
            fixture.command("apt-get", "exit 0");
            fixture.command("dpkg-query", "exit 0");
        }
        if program == "xbps-query" {
            fixture.command("xbps-install", "exit 0");
            fixture.command("xbps-remove", "exit 0");
        }
        let output = fixture.run_preview(&format!("{row}\tSource\tShown name\t1"));
        assert!(output.status.success(), "{program}: preview command failed");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("required command")
                || stdout.contains("provider failed")
                || stdout.contains("exited with"),
            "{program}: missing failure diagnostic: {stdout}"
        );
    }
}
