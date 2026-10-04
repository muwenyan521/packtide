use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "systide-orchestration-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir(&path).expect("create fixture directory");
        Self(path)
    }

    fn command(&self, name: &str, body: &str) {
        let path = self.0.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write fake command");
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .expect("make fake command executable");
    }

    fn path(&self) -> std::ffi::OsString {
        std::env::join_paths([self.0.as_path(), Path::new("/usr/bin"), Path::new("/bin")])
            .expect("build isolated PATH")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn list_data_skips_absent_optional_backends_after_native_success() {
    let fixture = Fixture::new();
    fixture.command("pacman", "exit 0");
    fixture.command("checkupdates", "printf 'native 1 -> 2\\n'");
    let output = Command::new(env!("CARGO_BIN_EXE_systide"))
        .args(["--list-data", "--ui-lang", "en"])
        .env("PATH", fixture.path())
        .output()
        .expect("run list-data with only native provider");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows = String::from_utf8_lossy(&output.stdout);
    assert!(rows.contains("native\tnative 1 -> 2"), "{rows}");
    assert!(rows.contains("native 1 -> 2"), "{rows}");
    assert!(
        !rows.contains("[Snap]"),
        "absent optional provider leaked: {rows}"
    );
    assert!(
        !rows.contains("[Brew]"),
        "absent optional provider leaked: {rows}"
    );
    assert!(
        !rows.contains("[Nix]"),
        "absent optional provider leaked: {rows}"
    );
}

#[test]
fn list_data_continues_after_multiple_optional_failures_and_keeps_native_rows() {
    let fixture = Fixture::new();
    fixture.command("pacman", "exit 0");
    fixture.command("checkupdates", "printf 'native 1 -> 2\\n'");
    for name in ["snap", "brew", "nix"] {
        fixture.command(
            name,
            &format!("printf '{name} fixture failed\\n' >&2; exit 9"),
        );
    }
    let output = Command::new(env!("CARGO_BIN_EXE_systide"))
        .args(["--list-data", "--ui-lang", "en"])
        .env("PATH", fixture.path())
        .output()
        .expect("run list-data with failed optional providers");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("native 1 -> 2"));
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    for backend in ["snap", "brew", "nix"] {
        assert!(
            diagnostics.contains(&format!("update diagnostics: {backend}:")),
            "missing {backend} diagnostics: {diagnostics}"
        );
    }
}
