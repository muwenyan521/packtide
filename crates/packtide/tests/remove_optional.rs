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
        let path = std::env::temp_dir().join(format!(
            "packtide-remove-optional-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create isolated fixture directory");
        Self(path)
    }

    fn command(&self, name: &str, body: &str) {
        let path = self.0.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}")).expect("write fake command");
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .expect("make fake command executable");
    }

    fn path(&self) -> std::ffi::OsString {
        std::env::join_paths([self.0.as_path(), Path::new("/usr/bin"), Path::new("/bin")])
            .expect("build isolated PATH")
    }

    fn run(&self) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_packtide"))
            .arg("remove")
            .env("PATH", self.path())
            .env("PACKTIDE_UI_LANG", "en")
            .env_remove("PACKTIDE_REMOVE_LIST_ONLY")
            .output()
            .expect("run packtide remove")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn install_common_commands(fixture: &Fixture, fzf_body: &str) {
    let flatpak = fixture.0.join("flatpak.argv");
    let snap = fixture.0.join("snap.argv");
    let brew = fixture.0.join("brew.argv");
    let nix = fixture.0.join("nix.argv");
    fixture.command(
        "pacman",
        "case \"$2\" in
-Q) printf 'native-installed 1\\n' ;;
-Qm) printf 'aur-installed\\n' ;;
*) exit 64 ;;
esac
",
    );
    fixture.command("paru", "exit 0\n");
    fixture.command(
        "flatpak",
        &format!(
            "case \"$1\" in
list) printf 'org.example.User\\tflathub\\tflat-user\\tuser\\norg.example.System\\tflathub\\tflat-system\\tsystem\\n' ;;
uninstall) printf '%s\\n' \"$*\" >> '{}' ;;
*) exit 64 ;;
esac
",
            flatpak.display()
        ),
    );
    fixture.command(
        "snap",
        &format!(
            "case \"$1\" in
list) printf 'Name Version Rev Tracking Publisher Notes\\nremove-snap 1 123 latest/stable canonical -\\n' ;;
remove) printf '%s\\n' \"$*\" >> '{}' ;;
*) exit 64 ;;
esac
",
            snap.display()
        ),
    );
    fixture.command(
        "brew",
        &format!(
            "case \"$1:$2\" in
info:--json=v2) printf '%s\\n' '{{\"formulae\":[{{\"name\":\"remove-brew\",\"full_name\":\"remove-brew\",\"installed_versions\":[\"1\"]}}]}}' ;;
uninstall:*) printf '%s\\n' \"$*\" >> '{}' ;;
*) exit 64 ;;
esac
",
            brew.display()
        ),
    );
    fixture.command(
        "nix",
        &format!(
            "case \"$3:$4\" in
profile:list) printf '%s\\n' '{{\"elements\":{{\"remove-nix\":{{\"attrPath\":\"remove-nix\",\"originalUrl\":\"nixpkgs\",\"storePaths\":[\"/nix/store/remove-nix\"],\"active\":true}}}}}}' ;;
profile:remove) printf '%s\\n' \"$*\" >> '{}' ;;
*) exit 64 ;;
esac
",
            nix.display()
        ),
    );
    fixture.command("fzf", fzf_body);
}

#[test]
fn remove_selects_optional_typed_rows_and_runs_scope_aware_transactions() {
    let fixture = Fixture::new();
    let rows = fixture.0.join("fzf.input");
    install_common_commands(
        &fixture,
        &format!(
            "cat > '{}'\ngrep -E 'flat-user|flat-system|remove-snap|remove-brew|remove-nix' '{}'\n",
            rows.display(),
            rows.display()
        ),
    );
    let output = fixture.run();
    assert!(
        output.status.success(),
        "remove failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let picker_rows = fs::read_to_string(&rows).expect("read rows sent to fzf");
    for name in [
        "flat-user",
        "flat-system",
        "remove-snap",
        "remove-brew",
        "remove-nix",
    ] {
        assert!(
            picker_rows.contains(name),
            "missing optional row {name}: {picker_rows}"
        );
    }
    assert_eq!(
        fs::read_to_string(fixture.0.join("flatpak.argv")).expect("read Flatpak argv"),
        "uninstall org.example.User\nuninstall --system org.example.System\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join("snap.argv")).expect("read Snap argv"),
        "remove remove-snap\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join("brew.argv")).expect("read Brew argv"),
        "uninstall remove-brew\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join("nix.argv")).expect("read Nix argv"),
        "--extra-experimental-features nix-command flakes profile remove remove-nix\n"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("scope=User"),
        "missing user transaction summary: {stdout}"
    );
    assert!(
        stdout.contains("scope=System"),
        "missing system transaction summary: {stdout}"
    );
}

#[test]
fn optional_provider_failure_is_diagnostic_and_does_not_abort_other_rows() {
    let fixture = Fixture::new();
    let rows = fixture.0.join("fzf.input");
    install_common_commands(
        &fixture,
        &format!(
            "cat > '{}'\ngrep -F 'remove-brew' '{}'\n",
            rows.display(),
            rows.display()
        ),
    );
    fixture.command("snap", "printf 'snap unavailable\\n' >&2; exit 23\n");
    let output = fixture.run();
    assert!(
        output.status.success(),
        "optional failure aborted remove: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("optional package providers unavailable"),
        "{stderr}"
    );
    assert!(stderr.contains("snap:"), "{stderr}");
    assert_eq!(
        fs::read_to_string(fixture.0.join("brew.argv")).expect("read surviving Brew argv"),
        "uninstall remove-brew\n"
    );
    assert!(!fixture.0.join("snap.argv").exists());
}
