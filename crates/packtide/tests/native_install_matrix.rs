#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new(os_release: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("packtide-native-install-{nonce}"));
        fs::create_dir(&root).expect("create fixture");
        fs::write(root.join("os-release"), os_release).expect("write os-release");
        Self(root)
    }

    fn command(&self, name: &str, body: &str) {
        let path = self.0.join(name);
        fs::write(path, format!("#!/bin/sh\n{body}\n")).expect("write command");
        fs::set_permissions(self.0.join(name), fs::Permissions::from_mode(0o755))
            .expect("chmod command");
    }

    fn run(&self) -> std::process::Output {
        let mut command = Command::new("bwrap");
        command
            .args(["--ro-bind", "/", "/", "--bind"])
            .arg(&self.0)
            .arg(&self.0)
            .args(["--ro-bind"])
            .arg(self.0.join("os-release"))
            .arg(fs::canonicalize("/etc/os-release").expect("host os-release"))
            .args([
                "--unshare-user",
                "--unshare-pid",
                "--proc",
                "/proc",
                "--dev",
                "/dev",
                "--setenv",
                "PATH",
            ])
            .arg(&self.0)
            .args(["--", env!("CARGO_BIN_EXE_packtide"), "install", "hello"])
            .env("PACKTIDE_INSTALL_LIST_ONLY", "1")
            .env("PACKTIDE_UI_LANG", "en");
        command.output().expect("run install list-only")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn native_install_catalog_matrix_emits_typed_hidden_rows() {
    let cases = [
        (
            "debian",
            "APT:apt",
            "apt-cache",
            "printf 'Package: hello\\nVersion: 1.0\\nArchitecture: amd64\\n\\n'",
        ),
        (
            "fedora",
            "DNF:dnf",
            "dnf5",
            "printf '[{\"name\":\"hello\",\"version\":\"1\",\"arch\":\"x86_64\"}]'",
        ),
        (
            "fedora",
            "DNF:dnf",
            "dnf",
            "printf 'hello\\t0\\t1\\trel\\tx86_64\\tbase\\t0\\n'",
        ),
        (
            "opensuse",
            "ZYPPER:zypper",
            "zypper",
            "printf '<root><solvable type=\"package\"><name>hello</name><version>1</version></solvable></root>'",
        ),
        (
            "alpine",
            "APK:apk",
            "apk",
            "printf 'hello\\t1.0-r0\\tx86_64\\tmain\\n'",
        ),
        (
            "void",
            "XBPS:xbps",
            "xbps-query",
            "printf 'hello-1.0_1\\trepo\\tx86_64\\n'",
        ),
    ];
    for (id, token, provider, body) in cases {
        let fixture = Fixture::new(&format!("ID={id}\n"));
        fixture.command(provider, body);
        if provider == "apt-cache" {
            fixture.command("apt-get", "exit 0");
            fixture.command("dpkg-query", "exit 0");
        }
        if provider == "xbps-query" {
            fixture.command("xbps-install", "exit 0");
            fixture.command("xbps-remove", "exit 0");
        }
        fixture.command("fzf", "cat >/dev/null; exit 1");
        let output = fixture.run();
        assert!(
            output.status.success(),
            "{id}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(token),
            "{id}: missing hidden source token: {stdout}"
        );
        assert!(
            stdout.contains("hello"),
            "{id}: missing package row: {stdout}"
        );
    }
}
