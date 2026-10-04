use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "packtide-optional-backends-{}-{}",
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

    fn path_with_system_tools(&self) -> std::ffi::OsString {
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
fn check_updates_keeps_snap_brew_and_nix_rows_when_one_optional_provider_fails() {
    let fixture = Fixture::new();
    fixture.command("pacman", "exit 0");
    fixture.command("checkupdates", "printf 'native 1 -> 2\\n'");
    fixture.command(
        "snap",
        "printf 'Name         Version  Rev  Publisher  Notes\\nhello-world  6.5      124  canonical  -\\n'",
    );
    fixture.command(
        "brew",
        "printf '%s\\n' '{\"formulae\":[{\"name\":\"brew-hello\",\"installed_versions\":[\"1\"],\"latest_version\":\"2\"}]}'",
    );
    fixture.command(
        "nix",
        "case \"$*\" in *' eval '*) printf '%s\\n' '\"/nix/store/new-hello\"' ;; *) printf '%s\\n' '{\"elements\":{\"hello\":{\"attrPath\":\"hello\",\"originalUrl\":\"nixpkgs\",\"storePaths\":[\"/nix/store/old-hello\"],\"active\":true}}}' ;; esac",
    );
    let path = fixture.path_with_system_tools();
    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["check-updates", "--refresh"])
        .env("PACKTIDE_UPDATE_LIST_ONLY", "1")
        .env("PACKTIDE_UI_LANG", "en")
        .env("XDG_CACHE_HOME", fixture.0.join("cache"))
        .env("PATH", path)
        .output()
        .expect("run optional provider list");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows = String::from_utf8_lossy(&output.stdout);
    assert!(rows.contains("hello-world ? -> 6.5"), "{rows}");
    assert!(rows.contains("brew-hello 1 -> 2"), "{rows}");
    assert!(
        rows.contains("hello /nix/store/old-hello -> /nix/store/new-hello"),
        "{rows}"
    );

    fixture.command("snap", "printf 'snapd unavailable\\n' >&2; exit 41");
    let failed = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["check-updates", "--refresh"])
        .env("PACKTIDE_UPDATE_LIST_ONLY", "1")
        .env("PACKTIDE_UI_LANG", "en")
        .env("XDG_CACHE_HOME", fixture.0.join("cache-failed"))
        .env("PATH", fixture.path_with_system_tools())
        .output()
        .expect("run optional provider failure");
    assert!(failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("update diagnostics: snap:"));
    assert!(String::from_utf8_lossy(&failed.stdout).contains("brew-hello"));
}

#[test]
fn optional_preview_uses_hidden_identity_for_snap_brew_and_nix() {
    let cases = [
        (
            "snap",
            "SNAP:snap|b=snap|s=system|k=snap|n=68656c6c6f\tSnap\thello\t6.5",
            "printf 'name: hello\\nsummary: fixture\\n'",
            "info hello\n",
        ),
        (
            "brew",
            "BREW:brew|b=brew|s=profile|k=brew-formula|n=68656c6c6f\tBrew\thello\t2",
            "printf '%s\\n' '{\"formulae\":[{\"name\":\"hello\",\"installed_versions\":[\"1\"],\"latest_version\":\"2\"}]}'",
            "info --json=v2 --formula hello\n",
        ),
        (
            "nix",
            "NIX:nix|b=nix|s=profile|k=nix|n=68656c6c6f\tNix\thello\t2",
            "printf '%s\\n' '{\"elements\":{\"hello\":{\"attrPath\":\"hello\",\"originalUrl\":\"nixpkgs\",\"storePaths\":[\"/nix/store/hello\"],\"active\":true}}}'",
            "--extra-experimental-features nix-command flakes profile list --json\n",
        ),
    ];
    for (backend, row, body, expected_args) in cases {
        let fixture = Fixture::new();
        let marker = fixture.0.join("argv");
        fixture.command(
            backend,
            &format!("printf '%s\\n' \"$*\" > '{}'\n{}", marker.display(), body),
        );
        let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
            .args(["__preview", "remove", row])
            .env("PACKTIDE_UI_LANG", "en")
            .env("PATH", fixture.path_with_system_tools())
            .output()
            .expect("run optional preview");
        assert!(
            output.status.success(),
            "{backend}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            marker.exists(),
            "{backend}: preview did not invoke provider; stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read_to_string(marker).unwrap(), expected_args);
    }
}
