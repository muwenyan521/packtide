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
            "packtide-install-optional-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create isolated fixture directory");
        Self(path)
    }
    fn write_executable(&self, name: &str, body: &str) {
        let path = self.0.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}")).expect("write fake command");
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .expect("make fake command executable");
    }
    fn isolated_path(&self) -> std::ffi::OsString {
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
fn install_selects_optional_typed_rows_and_runs_transactions() {
    let fixture = Fixture::new();
    let cache = fixture.0.join("cache");
    fs::create_dir_all(cache.join("packtide/aur")).expect("create isolated cache");
    fs::write(cache.join("packtide/aur/packages"), "aur-only\n").expect("seed AUR cache");
    let fzf_input = fixture.0.join("fzf.input");
    let flatpak_log = fixture.0.join("flatpak.argv");
    let snap_log = fixture.0.join("snap.argv");
    let brew_log = fixture.0.join("brew.argv");
    let nix_log = fixture.0.join("nix.argv");
    fixture.write_executable("pacman", "case \"$1:$2\" in --color=never:-Sl) printf 'core native-package 1\\n' ;; --color=never:-Qq|-Qq:|-S:) ;; *) exit 64 ;; esac\n");
    fixture.write_executable(
        "paru",
        "case \"$1\" in -Sl) printf 'aur aur-only 1\\n' ;; -S) ;; esac\n",
    );
    fixture.write_executable("flatpak", &format!("case \"$1\" in list) printf 'org.example.User\\tflathub\\tzz-flat-user\\tuser\\norg.example.System\\tflathub\\tzz-flat-system\\tsystem\\n' ;; install) printf '%s\\n' \"$*\" >> '{}' ;; *) exit 64 ;; esac\n", flatpak_log.display()));
    fixture.write_executable("snap", &format!("case \"$1\" in find) printf 'Name Version Publisher Notes Summary\\nzz-snap-choice 1 canonical - Snap choice\\n' ;; install) printf '%s\\n' \"$*\" >> '{}' ;; *) exit 64 ;; esac\n", snap_log.display()));
    fixture.write_executable("brew", &format!("case \"$1\" in search) printf 'zz-brew-choice\\n' ;; install) printf '%s\\n' \"$*\" >> '{}' ;; *) exit 64 ;; esac\n", brew_log.display()));
    fixture.write_executable("nix", &format!("case \"$3\" in search) printf '%s\\n' '{{\"nix-choice\":{{\"pname\":\"zz-nix-choice\"}}}}' ;; profile) printf '%s\\n' \"$*\" >> '{}' ;; *) exit 64 ;; esac\n", nix_log.display()));
    fixture.write_executable(
        "fzf",
        &format!(
            "cat > '{}'\ngrep -E 'zz-flat-user|zz-flat-system|Snap choice|zz-brew-choice|zz-nix-choice' '{}'\n",
            fzf_input.display(),
            fzf_input.display()
        ),
    );
    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["install", "zz"])
        .env("PATH", fixture.isolated_path())
        .env("XDG_CACHE_HOME", &cache)
        .env("PACKTIDE_UI_LANG", "en")
        .output()
        .expect("run install picker with fake providers");
    assert!(
        output.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows = fs::read_to_string(&fzf_input).expect("read provider rows sent to fzf");
    for source in ["FLTK:", "SNAP:", "BREW:", "NIX:"] {
        assert!(rows.contains(source), "missing typed {source} row: {rows}");
    }
    for name in [
        "zz-flat-user",
        "zz-flat-system",
        "Snap choice",
        "zz-brew-choice",
        "zz-nix-choice",
    ] {
        assert!(rows.contains(name), "missing provider row {name}: {rows}");
    }
    let flatpak_argv = fs::read_to_string(&flatpak_log).expect("read Flatpak transaction argv");
    assert!(
        flatpak_argv.contains("org.example.System"),
        "Flatpak argv: {flatpak_argv}"
    );
    assert_eq!(
        fs::read_to_string(&snap_log).expect("read Snap transaction argv"),
        "install zz-snap-choice\n"
    );
    assert_eq!(
        fs::read_to_string(&brew_log).expect("read Brew transaction argv"),
        "install zz-brew-choice\n"
    );
    assert_eq!(
        fs::read_to_string(&nix_log).expect("read Nix transaction argv"),
        "--extra-experimental-features nix-command flakes profile install nixpkgs#nix-choice\n"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("scope=User"),
        "missing user scope summary: {stdout}"
    );
    assert!(
        stdout.contains("scope=System"),
        "missing system scope summary: {stdout}"
    );
}
