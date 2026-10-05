#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "packtide-native-remove-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("os-release"), "ID=debian\n").unwrap();
        let fixture = Self(root);
        for name in ["apt-get", "apt-cache"] {
            fixture.command(name, "exit 0");
        }
        fixture.command(
            "dpkg-query",
            "printf 'native-tool\\t1.0\\tamd64\\tinstall ok installed\\n'",
        );
        fixture.command(
            "fzf",
            &format!("/bin/cat > '{}/picker.rows'\nexit 1", fixture.0.display()),
        );
        fixture
    }

    fn command(&self, name: &str, body: &str) {
        let path = self.0.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn run(&self, list_only: bool) -> Output {
        let mut command = Command::new("bwrap");
        command
            .args(["--ro-bind", "/", "/", "--bind"])
            .arg(&self.0)
            .arg(&self.0)
            .arg("--ro-bind")
            .arg(self.0.join("os-release"))
            .arg(fs::canonicalize("/etc/os-release").unwrap())
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
            .args(["--", env!("CARGO_BIN_EXE_packtide"), "remove"])
            .env("PACKTIDE_UI_LANG", "en")
            .env_remove("PACKTIDE_REMOVE_LIST_ONLY");
        if list_only {
            command.env("PACKTIDE_REMOVE_LIST_ONLY", "1");
        }
        let output = command.output().expect("run bubblewrap remove CLI");
        println!(
            "packtide remove list_only={list_only} status={}\nstdout={}\nstderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!("remove native fixture {}: {error}", self.0.display());
        }
    }
}

#[test]
#[ignore = "requires bubblewrap and Linux user namespaces; run with --ignored"]
fn native_remove_lists_installed_packages_without_pacman_or_aur_helper() {
    // Given: Debian with only its native tooling and fzf on PATH.
    let fixture = Fixture::new();

    // When: the real CLI prepares a list-only removal catalog.
    let output = fixture.run(true);

    // Then: the native identity is present without any Arch dependency.
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("APT:apt\t"), "{stdout}");
    assert!(stdout.contains("native-tool"), "{stdout}");
    assert!(!fixture.0.join("picker.rows").exists());
}

#[test]
#[ignore = "requires bubblewrap and Linux user namespaces; run with --ignored"]
fn native_remove_reaches_picker_without_pacman_or_aur_helper() {
    // Given: a native installed package and a picker that cancels.
    let fixture = Fixture::new();

    // When: the real CLI opens the removal picker.
    let output = fixture.run(false);

    // Then: cancellation succeeds after the native row reached fzf.
    assert!(output.status.success());
    let rows = fs::read_to_string(fixture.0.join("picker.rows")).unwrap();
    println!("picker.rows={rows}");
    assert!(rows.starts_with("APT:apt\t"), "{rows}");
    assert!(rows.contains("native-tool"), "{rows}");
}

#[test]
#[ignore = "requires bubblewrap and Linux user namespaces; run with --ignored"]
fn native_remove_reports_missing_provider_tool_without_requesting_pacman() {
    // Given: Debian's installed-query provider is unavailable.
    let fixture = Fixture::new();
    fs::remove_file(fixture.0.join("dpkg-query")).unwrap();

    // When: the real CLI requests installed rows.
    let output = fixture.run(true);

    // Then: failure identifies the native provider rather than Arch tooling.
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("native installed query failed"), "{stderr}");
    assert!(stderr.contains("dpkg-query"), "{stderr}");
    assert!(!stderr.contains("pacman"), "{stderr}");
    assert!(!fixture.0.join("picker.rows").exists());
}
