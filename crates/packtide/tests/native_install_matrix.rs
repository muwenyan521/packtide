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
        fs::create_dir(root.join("cache")).expect("create cache");
        fs::create_dir_all(root.join("cache")).expect("create cache");
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
            .arg("/usr/local/bin")
            .args(["--bind"])
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
            .arg("/usr/local/bin:/usr/bin:/bin")
            .args(["--setenv", "XDG_CACHE_HOME"])
            .arg(self.0.join("cache"))
            .args(["--", env!("CARGO_BIN_EXE_packtide"), "install", "hello"])
            .env("PACKTIDE_INSTALL_LIST_ONLY", "1")
            .env("PACKTIDE_UI_LANG", "en");
        command.output().expect("run install list-only")
    }

    fn run_install(&self) -> std::process::Output {
        let mut command = Command::new("bwrap");
        command
            .args(["--ro-bind", "/", "/", "--bind"])
            .arg(&self.0)
            .arg("/usr/local/bin")
            .args(["--bind"])
            .arg(&self.0)
            .arg(&self.0)
            .args(["--ro-bind"])
            .arg(self.0.join("os-release"))
            .arg(fs::canonicalize("/etc/os-release").expect("host os-release"))
            .args(["--bind"])
            .arg(self.0.join("sudo"))
            .arg("/usr/bin/sudo")
            .args(["--tmpfs", "/tmp"])
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
            .arg("/usr/local/bin:/usr/bin:/bin")
            .args(["--setenv", "XDG_CACHE_HOME"])
            .arg(self.0.join("cache"))
            .args(["--setenv", "HOME"])
            .arg(&self.0)
            .args(["--", env!("CARGO_BIN_EXE_packtide"), "install", "hello"])
            .env("PACKTIDE_UI_LANG", "en");
        command.output().expect("run install picker")
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

#[test]
fn native_install_picker_selects_row_and_runs_transaction_argv() {
    let cases = [
        (
            "debian",
            "apt-cache",
            "apt-get",
            "printf 'Package: hello\\nVersion: 1.0\\nArchitecture: amd64\\n\\n'",
            "install hello",
        ),
        (
            "fedora",
            "dnf5",
            "dnf5",
            "printf '[{\"name\":\"hello\",\"version\":\"1\",\"arch\":\"x86_64\"}]'",
            "install hello",
        ),
        (
            "fedora",
            "dnf",
            "dnf",
            "printf 'hello\\t0\\t1\\trel\\tx86_64\\tbase\\t0\\n'",
            "install hello",
        ),
        (
            "opensuse",
            "zypper",
            "zypper",
            "printf '<root><solvable type=\"package\"><name>hello</name><version>1</version></solvable></root>'",
            "install hello",
        ),
        (
            "alpine",
            "apk",
            "apk",
            "printf 'hello\\t1.0-r0\\tx86_64\\tmain\\n'",
            "add hello",
        ),
        (
            "void",
            "xbps-query",
            "xbps-install",
            "printf 'hello-1.0_1\\trepo\\tx86_64\\n'",
            "-y hello",
        ),
    ];
    for (id, catalog, transaction, body, expected) in cases {
        let fixture = Fixture::new(&format!("ID={id}\n"));
        fixture.command(catalog, body);
        if catalog == "apt-cache" {
            fixture.command(
                "apt-get",
                &format!(
                    "printf '%s\\n' \"$*\" >> '{}/transaction.argv'",
                    "/usr/local/bin"
                ),
            );
            fixture.command("dpkg-query", "exit 0");
        }
        if catalog == "xbps-query" {
            fixture.command(
                "xbps-install",
                &format!(
                    "printf '%s\\n' \"$*\" >> '{}/transaction.argv'",
                    "/usr/local/bin"
                ),
            );
            fixture.command("xbps-remove", "exit 0");
        }
        if transaction != "apt-get" && transaction != "xbps-install" {
            fixture.command(transaction, &format!("case \"$1\" in -y|-S|install|add) printf '%s\\n' \"$*\" >> '/usr/local/bin/transaction.argv' ;; *) {} ;; esac", body));
        }
        let rows = fixture.0.join("fzf.input");
        fixture.command(
            "fzf",
            "tee /usr/local/bin/fzf.input >/dev/null; sed -n '1p' /usr/local/bin/fzf.input",
        );
        fixture.command(
            "sudo",
            &format!(
                "printf 'sudo %s\\n' \"$*\" >> '{}/sudo.argv'; exec \"$@\"",
                fixture.0.display()
            ),
        );
        let output = fixture.run_install();
        assert!(
            output.status.success(),
            "{id}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let selected = fs::read_to_string(&rows).expect("fzf input");
        assert!(
            selected.contains("hello"),
            "{id}: missing selected row: {selected}"
        );
        let argv =
            fs::read_to_string(fixture.0.join("transaction.argv")).expect("transaction argv");
        assert_eq!(argv.trim(), expected, "{id}: transaction argv");
    }
}

#[test]
fn native_install_reports_provider_and_transaction_failures() {
    let provider = Fixture::new("ID=debian\n");
    provider.command("apt-cache", "printf 'catalog failed\\n' >&2; exit 23");
    provider.command("apt-get", "exit 0");
    provider.command("dpkg-query", "exit 0");
    provider.command("fzf", "cat >/dev/null; exit 1");
    let output = provider.run_install();
    assert!(!output.status.success());
    let provider_diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !provider_diagnostics.trim().is_empty(),
        "provider diagnostics missing"
    );

    let transaction = Fixture::new("ID=debian\n");
    transaction.command(
        "apt-cache",
        "printf 'Package: hello\\nVersion: 1.0\\nArchitecture: amd64\\n\\n'",
    );
    transaction.command("apt-get", "printf 'transaction failed\\n' >&2; exit 37");
    transaction.command("dpkg-query", "exit 0");
    transaction.command(
        "fzf",
        "tee /tmp/packtide-fzf-failure >/dev/null; sed -n '1p' /tmp/packtide-fzf-failure",
    );
    transaction.command("sudo", "exec \"$@\"");
    let output = transaction.run_install();
    assert!(!output.status.success());
    let transaction_diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(transaction_diagnostics.contains("apt-get") || transaction_diagnostics.contains("37"));
}
