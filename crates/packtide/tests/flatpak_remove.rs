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
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let base = std::env::temp_dir();
        let mut path = base.join(format!(
            "packtide-flatpak-remove-{}-{nonce}",
            std::process::id()
        ));
        let mut suffix = 0u32;
        loop {
            match fs::create_dir(&path) {
                Ok(()) => break,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    suffix += 1;
                    path = base.join(format!(
                        "packtide-flatpak-remove-{}-{nonce}-{suffix}",
                        std::process::id()
                    ));
                }
                Err(error) => panic!("create isolated fake-command directory: {error}"),
            }
        }
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write_executable(&self, name: &str, body: &str) {
        let path = self.0.join(name);
        fs::write(&path, body).expect("write fake command");
        let mut permissions = fs::metadata(&path)
            .expect("stat fake command")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("make fake command executable");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!(
                "cannot remove fake-command fixture {}: {error}",
                self.0.display()
            );
        }
    }
}

#[test]
fn flatpak_remove_passes_the_application_id_to_uninstall() {
    let fixture = Fixture::new();
    let fzf_input = fixture.path().join("fzf-input.tsv");
    let fzf_args = fixture.path().join("fzf.args");
    let flatpak_argv = fixture.path().join("flatpak.argv");
    let paru_argv = fixture.path().join("paru.argv");

    fixture.write_executable("pacman", "#!/bin/sh\nexit 0\n");
    fixture.write_executable(
        "flatpak",
        &format!(
            "#!/bin/sh\ncase \"$1\" in\n  list) printf 'org.example.App\\tflathub\\tExample App\\n' ;;\n  uninstall) printf '%s\\n' \"$@\" > '{}' ;;\n  *) exit 64 ;;\nesac\n",
            flatpak_argv.display()
        ),
    );
    fixture.write_executable(
        "fzf",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{1}'\ncat > '{0}'\n/usr/bin/awk -F '\\t' '$0 ~ /org.example.App/ {{ print; found = 1; exit }} END {{ if (!found) exit 4 }}' '{0}'\n",
            fzf_input.display(),
            fzf_args.display()
        ),
    );
    fixture.write_executable(
        "paru",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 0\n",
            paru_argv.display()
        ),
    );
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");

    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["remove"])
        .env("PATH", path)
        .env("PACKTIDE_UI_LANG", "zh")
        .output()
        .expect("run packtide remove with fake commands");

    assert!(
        output.status.success(),
        "packtide remove failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows = fs::read_to_string(fzf_input).expect("read rows sent to fake fzf");
    let arguments = fs::read_to_string(fzf_args).expect("read remove picker arguments");
    assert!(arguments.contains("PACKTIDE · 卸载软件包"));
    assert!(!arguments.contains("PACKTIDE · 安装软件包"));
    assert!(arguments.contains("\x1b[1;33mPACKTIDE · 卸载软件包\x1b[0m"));
    assert!(arguments.contains("! 卸载不可逆"));
    assert!(arguments.contains("alt-c:accept"));
    assert!(arguments.contains("正在刷新卸载列表"));
    assert!(arguments.contains("__preview remove"));
    assert!(rows.contains("\x1b[36mflatpak"));
    assert!(rows.contains("org.example.App"));
    assert!(rows.contains("Example App (flathub)"));
    assert_eq!(
        fs::read_to_string(flatpak_argv).expect("read Flatpak argv"),
        "uninstall\norg.example.App\n"
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(
        "transaction: action=remove helper=flatpak privilege=direct targets=org.example.App"
    ));
    assert!(!paru_argv.exists());
}

#[test]
fn remove_cancel_keeps_all_transactions_unstarted() {
    let fixture = Fixture::new();
    let paru_argv = fixture.path().join("paru.argv");
    let flatpak_argv = fixture.path().join("flatpak.argv");

    fixture.write_executable(
        "pacman",
        "#!/bin/sh\ncase \"$2\" in\n  -Q) printf 'bash 5.3-1\\n' ;;\n  -Sl) printf 'core bash 5.3-1\\n' ;;\n  *) exit 64 ;;\nesac\n",
    );
    fixture.write_executable(
        "paru",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 0\n",
            paru_argv.display()
        ),
    );
    fixture.write_executable(
        "flatpak",
        &format!(
            "#!/bin/sh\ncase \"$1\" in list) exit 0 ;; uninstall) printf '%s\\n' \"$@\" > '{}' ;; *) exit 64 ;; esac\n",
            flatpak_argv.display()
        ),
    );
    fixture.write_executable("fzf", "#!/bin/sh\ncat > /dev/null\nexit 1\n");
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");

    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["remove"])
        .env("PATH", path)
        .env("PACKTIDE_UI_LANG", "en")
        .output()
        .expect("run packtide remove with fake fzf cancellation");

    assert!(
        output.status.success(),
        "remove cancellation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("No packages selected"));
    assert!(!paru_argv.exists());
    assert!(!flatpak_argv.exists());
}

#[test]
fn default_install_keeps_query_when_no_ai_flag_follows_it() {
    let fixture = Fixture::new();
    let fzf_args = fixture.path().join("fzf.argv");
    let paru_args = fixture.path().join("paru.argv");
    let cache = fixture.path().join("cache");
    let aur_cache = cache.join("packtide/aur");
    fs::create_dir_all(&aur_cache).expect("create fresh AUR cache fixture");
    fs::write(aur_cache.join("packages"), "").expect("write fresh empty AUR cache");

    fixture.write_executable(
        "pacman",
        "#!/bin/sh\ncase \"$1:$2\" in\n  --color=never:-Sl) printf 'core bash 5.3-1\\n' ;;\n  -Qq:) printf 'bash\\n' ;;\n  *) exit 64 ;;\nesac\n",
    );
    fixture.write_executable(
        "paru",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 0\n",
            paru_args.display()
        ),
    );
    fixture.write_executable(
        "fzf",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\ncat > /dev/null\nexit 1\n",
            fzf_args.display()
        ),
    );
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");
    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["bash", "--no-ai"])
        .env("PATH", path)
        .env("XDG_CACHE_HOME", cache)
        .env("PACKTIDE_UI_LANG", "en")
        .output()
        .expect("run default install entry with fake commands");

    assert!(
        output.status.success(),
        "packtide install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let arguments = fs::read_to_string(fzf_args).expect("read fake fzf argv");
    assert!(arguments.contains("PACKTIDE · Install Packages"));
    assert!(arguments.contains("Packages to install >"));
    assert!(arguments.contains("--pointer"));
    assert!(arguments.contains("▌"));
    assert!(arguments.contains("--marker"));
    assert!(arguments.contains("✔"));
    let arguments = arguments.lines().collect::<Vec<_>>();
    let query_index = arguments
        .iter()
        .position(|argument| *argument == "--query")
        .expect("fzf query option");
    assert_eq!(arguments.get(query_index + 1), Some(&"bash"));
    assert!(!arguments.contains(&"--no-ai"));
    assert!(!paru_args.exists());
}

#[test]
fn install_selection_preserves_repository_and_aur_transaction_arguments() {
    let fixture = Fixture::new();
    let paru_argv = fixture.path().join("paru.argv");
    let cache = fixture.path().join("cache");
    let aur_cache = cache.join("packtide/aur");
    fs::create_dir_all(&aur_cache).expect("create fresh AUR cache fixture");
    fs::write(aur_cache.join("packages"), "aur-tool\n").expect("write AUR cache fixture");

    fixture.write_executable(
        "pacman",
        "#!/bin/sh\ncase \"$1:$2\" in\n  --color=never:-Sl) printf 'core bash 5.3-1\\n' ;;\n  -Qq:) exit 0 ;;\n  *) exit 64 ;;\nesac\n",
    );
    fixture.write_executable(
        "paru",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nexit 0\n",
            paru_argv.display()
        ),
    );
    fixture.write_executable("fzf", "#!/bin/sh\n/usr/bin/awk '/bash/ || /aur-tool/'\n");
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");

    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["install"])
        .env("PATH", path)
        .env("XDG_CACHE_HOME", cache)
        .output()
        .expect("run packtide install with fake commands");

    assert!(
        output.status.success(),
        "packtide install failed; stderr: {}; stdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::read_to_string(paru_argv).expect("read fake paru argv"),
        "-S\ncore/bash\n-S\naur/aur-tool\n"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(
        "transaction: action=install helper=paru privilege=helper-managed targets=core/bash"
    ));
    assert!(stdout.contains(
        "transaction: action=install helper=paru privilege=helper-managed targets=aur/aur-tool"
    ));
}

#[test]
fn install_picker_starts_before_package_sources_finish() {
    let fixture = Fixture::new();
    let fzf_started = fixture.path().join("fzf.started");
    let pacman_finished = fixture.path().join("pacman.finished");
    let cache = fixture.path().join("cache");
    let aur_cache = cache.join("packtide/aur");
    fs::create_dir_all(&aur_cache).expect("create AUR cache fixture");
    fs::write(aur_cache.join("packages"), "").expect("write AUR cache fixture");

    fixture.write_executable(
        "pacman",
        &format!(
            "#!/bin/sh\ncase \"$1:$2\" in\n  --color=never:-Sl) /usr/bin/sleep 0.3; : > '{}' ; printf 'core bash 5.3-1\\n' ;;\n  -Qq:) exit 0 ;;\n  *) exit 64 ;;\nesac\n",
            pacman_finished.display()
        ),
    );
    fixture.write_executable("paru", "#!/bin/sh\nexit 0\n");
    fixture.write_executable(
        "fzf",
        &format!(
            "#!/bin/sh\n: > '{0}'\nprintf '%s\\n' \"$@\" > '{0}.argv'\n/bin/cat > /dev/null\nexit 1\n",
            fzf_started.display()
        ),
    );
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");

    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["install"])
        .env("PATH", path)
        .env("XDG_CACHE_HOME", cache)
        .output()
        .expect("run packtide with delayed package source");

    assert!(
        output.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let picker_started = fs::metadata(&fzf_started)
        .expect("fzf should start")
        .modified()
        .expect("read fzf start timestamp");
    let source_finished = fs::metadata(&pacman_finished)
        .expect("pacman source should finish")
        .modified()
        .expect("read pacman completion timestamp");
    assert!(picker_started < source_finished);
    let args =
        fs::read_to_string(fzf_started.with_extension("started.argv")).expect("read fzf argv");
    assert!(!args.contains("start:reload"));
}

#[test]
fn install_falls_back_to_yay_when_paru_is_unavailable() {
    let fixture = Fixture::new();
    let yay_argv = fixture.path().join("yay.argv");
    let cache = fixture.path().join("cache");
    let aur_cache = cache.join("packtide/aur");
    fs::create_dir_all(&aur_cache).expect("create fresh AUR cache fixture");
    fs::write(aur_cache.join("packages"), "").expect("write fresh empty AUR cache");
    let paru = fixture.path().join("paru");
    fs::write(&paru, "not executable").expect("create non-executable paru decoy");
    let mut paru_permissions = fs::metadata(&paru)
        .expect("stat non-executable paru decoy")
        .permissions();
    paru_permissions.set_mode(0o644);
    fs::set_permissions(&paru, paru_permissions).expect("keep paru decoy non-executable");

    fixture.write_executable(
        "pacman",
        "#!/bin/sh\ncase \"$1:$2\" in\n  --color=never:-Sl) printf 'core bash 5.3-1\\n' ;;\n  -Qq:) exit 0 ;;\n  *) exit 64 ;;\nesac\n",
    );
    fixture.write_executable(
        "yay",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 0\n",
            yay_argv.display()
        ),
    );
    fixture.write_executable("fzf", "#!/bin/sh\n/bin/cat\n");

    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["install"])
        .env("PATH", fixture.path())
        .env("XDG_CACHE_HOME", cache)
        .output()
        .expect("run packtide install with fake yay");

    assert!(
        output.status.success(),
        "packtide install failed; stderr: {}; stdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::read_to_string(yay_argv).expect("read fake yay argv"),
        "-S\ncore/bash\n"
    );
}

#[test]
fn install_missing_fzf_names_the_affected_picker() {
    assert_missing_required_command("install", "fzf", "the package installation picker");
}

#[test]
fn install_missing_pacman_names_the_affected_lookup() {
    assert_missing_required_command_after_stubs(
        "install",
        "pacman",
        "the package catalog lookup",
        &["fzf"],
    );
}

#[test]
fn remove_missing_fzf_names_the_affected_picker() {
    assert_missing_required_command("remove", "fzf", "the package removal picker");
}

#[test]
fn remove_missing_pacman_names_the_affected_lookup() {
    assert_missing_required_command_after_stubs(
        "remove",
        "pacman",
        "the installed package lookup",
        &["fzf"],
    );
}

#[test]
fn install_missing_aur_helper_names_the_affected_operation() {
    assert_missing_aur_helper("install", "package installation");
}

#[test]
fn remove_missing_aur_helper_names_the_affected_operation() {
    assert_missing_aur_helper("remove", "package removal");
}

#[test]
fn downgrade_missing_aur_helper_names_the_affected_operation() {
    assert_missing_aur_helper_after_stubs(
        "downgrade",
        "downgrade package details",
        &["fzf", "pacman", "downgrade"],
    );
}

#[test]
fn downgrade_missing_fzf_names_the_affected_picker() {
    assert_missing_required_command("downgrade", "fzf", "the package downgrade picker");
}

#[test]
fn downgrade_missing_pacman_names_the_affected_lookup() {
    assert_missing_required_command_after_stubs(
        "downgrade",
        "pacman",
        "downgrade package information",
        &["fzf"],
    );
}

#[test]
fn downgrade_missing_transaction_tool_names_the_affected_operation() {
    assert_missing_required_command_after_stubs(
        "downgrade",
        "downgrade",
        "the package downgrade transaction",
        &["fzf", "pacman"],
    );
}

#[test]
fn downgrade_cancel_does_not_start_the_transaction() {
    let fixture = Fixture::new();
    let fzf_input = fixture.path().join("fzf.input");
    let downgrade_argv = fixture.path().join("downgrade.argv");
    let paru_argv = fixture.path().join("paru.argv");
    fixture.write_executable(
        "pacman",
        "#!/bin/sh\ncase \"$2\" in\n  -Sl) printf 'core bash 5.3-1\\n' ;;\n  -Q) printf 'bash 5.3-1\\n' ;;\n  *) exit 64 ;;\nesac\n",
    );
    fixture.write_executable(
        "paru",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nexit 0\n",
            paru_argv.display()
        ),
    );
    fixture.write_executable(
        "downgrade",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 0\n",
            downgrade_argv.display()
        ),
    );
    fixture.write_executable(
        "fzf",
        &format!("#!/bin/sh\n/bin/cat > '{}'\nexit 1\n", fzf_input.display()),
    );

    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["downgrade"])
        .env("PATH", fixture.path())
        .env("PACKTIDE_UI_LANG", "en")
        .output()
        .expect("run packtide downgrade with a fake fzf cancel");

    assert!(
        output.status.success(),
        "packtide downgrade failed; stderr: {}; stdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("No packages selected for downgrade.")
    );
    let rows = fs::read_to_string(fzf_input).expect("read rows sent to fake fzf");
    assert!(rows.contains("core"));
    assert!(rows.contains("bash"));
    assert!(!downgrade_argv.exists());
    assert!(!paru_argv.exists());
}

#[test]
fn downgrade_picker_uses_localized_header_and_source_colors() {
    let fixture = Fixture::new();
    let fzf_args = fixture.path().join("fzf.args");
    fixture.write_executable(
        "pacman",
        "#!/bin/sh\ncase \"$2\" in\n  -Sl) printf 'core bash 5.3-1\\n' ;;\n  -Q) printf 'bash 5.3-1\\naur-tool 1.0\\n' ;;\n  *) exit 64 ;;\nesac\n",
    );
    fixture.write_executable("paru", "#!/bin/sh\nexit 0\n");
    fixture.write_executable("downgrade", "#!/bin/sh\nexit 0\n");
    fixture.write_executable(
        "fzf",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\ncat > /dev/null\nexit 1\n",
            fzf_args.display()
        ),
    );
    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["downgrade"])
        .env("PATH", fixture.path())
        .env("PACKTIDE_UI_LANG", "en")
        .output()
        .expect("run localized downgrade picker");

    assert!(output.status.success());
    let args = fs::read_to_string(fzf_args).expect("read fzf arguments");
    assert!(args.contains("PACKTIDE · Downgrade Packages"));
    assert!(args.contains("Packages to downgrade >"));
    assert!(args.contains("__preview downgrade"));
}

#[test]
fn mirror_update_missing_reflector_names_the_affected_operation() {
    assert_missing_required_command("mirror-update", "reflector", "the mirror list update");
}

#[test]
fn sysup_compatibility_bridge_reports_systide_dependency() {
    let fixture = Fixture::new();
    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["sysup"])
        .env("PATH", fixture.path())
        .output()
        .expect("run packtide sysup with no package helpers");
    let error = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(error.contains("no AUR helper found") || error.contains("required command 'systide'"));
}

fn assert_missing_required_command(subcommand: &str, command: &str, capability: &str) {
    assert_missing_required_command_after_stubs(subcommand, command, capability, &[]);
}

fn assert_missing_required_command_after_stubs(
    subcommand: &str,
    command: &str,
    capability: &str,
    available_commands: &[&str],
) {
    let fixture = Fixture::new();
    for available in available_commands {
        fixture.write_executable(available, "#!/bin/sh\nexit 0\n");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args([subcommand])
        .env("PATH", fixture.path())
        .output()
        .expect("run packtide with an isolated empty PATH");
    let error = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(
        error.contains(&format!(
            "required command '{command}' is unavailable for {capability}"
        )),
        "missing dependency error lacked command or capability context: {error}"
    );
    assert!(error.contains("install it and retry"));
}

fn assert_missing_aur_helper(subcommand: &str, capability: &str) {
    assert_missing_aur_helper_after_stubs(subcommand, capability, &["fzf", "pacman"]);
}

fn assert_missing_aur_helper_after_stubs(
    subcommand: &str,
    capability: &str,
    available_commands: &[&str],
) {
    let fixture = Fixture::new();
    for available in available_commands {
        fixture.write_executable(available, "#!/bin/sh\nexit 0\n");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args([subcommand])
        .env("PATH", fixture.path())
        .output()
        .expect("run packtide with no AUR helper");
    let error = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(error.contains("required AUR helper ('paru' or 'yay')"));
    assert!(error.contains(&format!("for {capability}")));
    assert!(error.contains("install paru or yay and retry"));
}
