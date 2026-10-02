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
        let path = std::env::temp_dir().join(format!(
            "packtide-remove-picker-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn command(&self, name: &str, body: &str) {
        let path = self.0.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn run(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_packtide"))
            .arg("remove")
            .env("PATH", &self.0)
            .env("PACKTIDE_UI_LANG", "en")
            .env_remove("PACKTIDE_REMOVE_LIST_ONLY")
            .output()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!("remove fixture {}: {error}", self.0.display());
        }
    }
}

fn assert_accept_routes_to_backend(helper: &str, target: &str, backend: &str) {
    // Given: both official and foreign rows, with no host commands on PATH.
    let fixture = Fixture::new();
    fixture.command("pacman", &format!(
        "#!/bin/sh\ncase \"$1:$2\" in\n --color=never:-Q) printf 'official 1.0\\naur-tool 1.0\\n' ;;\n --color=never:-Qm) printf 'aur-tool\\n' ;;\n -Rns:*) printf '%s\\n' \"$@\" > '{0}/pacman.argv' ;;\n *) exit 64 ;;\nesac\n", fixture.0.display()));
    fixture.command(
        helper,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{0}/{helper}.argv'\n",
            fixture.0.display()
        ),
    );
    fixture.command(
        "fzf",
        &format!(
            "#!/bin/sh\n/bin/cat > '{0}/picker.rows'\n/bin/grep -F '{target}' '{0}/picker.rows'\n",
            fixture.0.display()
        ),
    );

    // When: the real remove entry accepts a generated picker row.
    let output = fixture.run();

    // Then: only the identity's backend receives the bare native key.
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join(format!("{backend}.argv"))).unwrap(),
        format!("-Rns\n{target}\n")
    );
    let other = if backend == "pacman" {
        helper
    } else {
        "pacman"
    };
    assert!(!fixture.0.join(format!("{other}.argv")).exists());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let privilege = if backend == "pacman" {
        "direct"
    } else {
        "helper-managed"
    };
    assert!(stdout.contains(&format!(
        "transaction: action=remove helper={backend} privilege={privilege} targets={target}"
    )));
}

#[test]
fn official_picker_accept_invokes_direct_pacman_when_paru_is_selected() {
    assert_accept_routes_to_backend("paru", "official", "pacman");
}

#[test]
fn aur_picker_accept_invokes_paru_when_paru_is_selected() {
    assert_accept_routes_to_backend("paru", "aur-tool", "paru");
}

#[test]
fn aur_picker_accept_invokes_yay_when_only_yay_is_available() {
    assert_accept_routes_to_backend("yay", "aur-tool", "yay");
}

fn assert_foreign_read_failure(helper: &str) {
    // Given: the installed query succeeds but the foreign query fails.
    let fixture = Fixture::new();
    fixture.command("pacman", "#!/bin/sh\ncase \"$2\" in\n -Q) printf 'aur-tool 1.0\\n' ;;\n -Qm) printf 'foreign-db-failure\\n' >&2; exit 23 ;;\n *) exit 64 ;;\nesac\n");
    fixture.command(helper, "#!/bin/sh\nexit 0\n");
    fixture.command(
        "fzf",
        &format!(
            "#!/bin/sh\n/bin/cat > '{}/picker.rows'\nexit 1\n",
            fixture.0.display()
        ),
    );

    // When: removal prepares its installed rows.
    let output = fixture.run();

    // Then: a source failure aborts before the picker rather than relabeling AUR.
    assert!(!output.status.success(), "foreign read failure was masked");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("{helper} typed installed read failed")),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!(
            "backend {helper} failed to list foreign installed packages: foreign-db-failure"
        )),
        "{stderr}"
    );
    assert!(!fixture.0.join("picker.rows").exists());
}

#[test]
fn paru_installed_failure_aborts_before_picker() {
    assert_foreign_read_failure("paru");
}

#[test]
fn yay_installed_failure_aborts_before_picker() {
    assert_foreign_read_failure("yay");
}

#[test]
fn foreign_read_failure_is_not_masked_when_selected_helper_disappears() {
    // Given: helper selection succeeds, then the installed query removes it.
    let fixture = Fixture::new();
    fixture.command("pacman", &format!("#!/bin/sh\ncase \"$2\" in\n -Q) /bin/rm '{}/paru'; printf 'aur-tool 1.0\\n' ;;\n -Qm) printf 'foreign-db-failure\\n' >&2; exit 23 ;;\n *) exit 64 ;;\nesac\n", fixture.0.display()));
    fixture.command("paru", "#!/bin/sh\nexit 0\n");
    fixture.command(
        "fzf",
        &format!(
            "#!/bin/sh\n/bin/cat > '{}/picker.rows'\nexit 1\n",
            fixture.0.display()
        ),
    );

    // When: removal prepares its installed rows after helper resolution.
    let output = fixture.run();

    // Then: disappearance cannot bypass the mandatory typed foreign query.
    assert!(
        !output.status.success(),
        "missing helper masked foreign read failure"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("paru typed installed read failed"));
    assert!(!fixture.0.join("picker.rows").exists());
}
