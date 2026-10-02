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
        .env("COLUMNS", "30")
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
    assert!(arguments.contains("\x1b[33m使用 paru\x1b[0m"));
    assert!(arguments.contains("! 卸载不可逆"));
    assert!(arguments.contains("Tab:多选"));
    assert!(arguments.contains("Enter:卸载"));
    assert!(arguments.contains("Ctrl+R:强制刷新"));
    assert!(arguments.contains("Esc:退出"));
    assert!(arguments.contains("alt-c:accept"));
    assert!(arguments.contains("正在刷新卸载列表"));
    assert!(arguments.contains("__preview remove"));
    assert!(rows.contains("\x1b[36mFlatpak"));
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
        .env("COLUMNS", "40")
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
    assert!(arguments.contains("Tab:select"));
    assert!(arguments.contains("Enter:install"));
    assert!(arguments.contains("Ctrl+R:refresh"));
    assert!(arguments.contains("Esc:exit"));
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
fn install_fake_picker_covers_preview_accept_and_transaction_argv() {
    let fixture = Fixture::new();
    let fzf_args = fixture.path().join("fzf.args");
    let preview_output = fixture.path().join("preview.out");
    let row_output = fixture.path().join("row.out");
    let command_output = fixture.path().join("command.out");
    let transaction_argv = fixture.path().join("paru.transaction.argv");
    let cache = fixture.path().join("cache");
    let aur_cache = cache.join("packtide/aur");
    fs::create_dir_all(&aur_cache).expect("create AUR cache fixture");
    fs::write(aur_cache.join("packages"), "").expect("write empty AUR cache");

    fixture.write_executable(
        "pacman",
        "#!/bin/sh\ncase \"$1:$2\" in\n  --color=never:-Sl) printf 'core bash 5.3-1\\n' ;;\n  *) printf 'Name : bash\\nVersion : 5.3-1\\nDescription : shell\\n' ;;\nesac\n",
    );
    fixture.write_executable(
        "paru",
        &format!(
            "#!/bin/sh\ncase \"$1:$2\" in\n  --color=always:-Si) printf 'Name : bash\\nVersion : 5.3-1\\nDescription : shell\\n' ;;\n  -S:core/bash) printf '%s\\n' \"$@\" > '{}' ;;\n  *) exit 64 ;;\nesac\n",
            transaction_argv.display()
        ),
    );
    fixture.write_executable(
        "fzf",
        &format!(
            "#!/bin/bash\nprintf '%s\\n' \"$@\" > '{}'\ninput=$(cat)\nrow=$(printf '%s\\n' \"$input\" | /usr/bin/awk '/bash/{{print; exit}}')\nprintf '%s\\n' \"$row\" > '{}'\npreview=''\nexpect_preview=0\nfor arg in \"$@\"; do\n  if [ \"$expect_preview\" = 1 ]; then preview=\"$arg\"; expect_preview=0; elif [ \"$arg\" = \"--preview\" ]; then expect_preview=1; fi\ndone\nprintf '%s\\n' \"$preview\" > '{}'\nprintf -v quoted '%q' \"$row\"\nplaceholder='\"{{}}\"'\ncommand=\"${{preview//$placeholder/$quoted}}\"\nprintf '%s\\n' \"$command\" >> '{}'\n/bin/bash -c \"$command\" > '{}' 2>&1\nprintf '%s\\n' \"$row\"\n",
            fzf_args.display(),
            row_output.display(),
            command_output.display(),
            command_output.display(),
            preview_output.display()
        ),
    );
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");

    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["install"])
        .env("PATH", path)
        .env("XDG_CACHE_HOME", cache)
        .env("PACKTIDE_UI_LANG", "en")
        .output()
        .expect("run install with full fake picker path");

    assert!(
        output.status.success(),
        "install failed; stderr: {}; stdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let args = fs::read_to_string(fzf_args).expect("read fake fzf argv");
    assert!(args.contains("--preview"));
    assert!(args.contains("__preview install"));
    assert!(args.contains("--no-wrap"));
    assert!(args.contains("--ellipsis=..."));
    assert!(args.contains("alt-j:last,alt-k:first"));
    assert!(args.contains("change:first"));
    assert!(args.contains("load:change-prompt(Packages to install >)"));
    let row = fs::read_to_string(row_output).expect("read selected row");
    let command = fs::read_to_string(command_output).expect("read preview command");
    let preview = fs::read_to_string(preview_output).expect("read preview output");
    assert!(
        preview.contains("bash"),
        "row={row:?} command={command:?} preview output was {preview:?}"
    );
    assert_eq!(
        fs::read_to_string(transaction_argv).expect("read transaction argv"),
        "-S\ncore/bash\n"
    );
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

fn write_update_cache(cache: &Path) {
    let path = cache.join("packtide/check-updates");
    fs::create_dir_all(&path).expect("create update cache");
    fs::write(
        path.join("updates.txt"),
        "pacman\tbash 5.3-1 -> 5.4-1\naur\taur-tool 1.0 -> 1.1",
    )
    .expect("write update cache");
}

#[test]
fn check_updates_fake_picker_preserves_order_and_cancel_feedback() {
    let fixture = Fixture::new();
    let cache = fixture.path().join("cache");
    let fzf_input = fixture.path().join("check-updates.input");
    let fzf_args = fixture.path().join("check-updates.args");
    write_update_cache(&cache);
    fixture.write_executable("flatpak", "#!/bin/sh\nprintf 'org.example.App 2.0\\n'\n");
    fixture.write_executable(
        "fzf",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\ncat > '{}'\nexit 1\n",
            fzf_args.display(),
            fzf_input.display()
        ),
    );
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");

    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["check-updates"])
        .env("PATH", path)
        .env("XDG_CACHE_HOME", cache)
        .env("PACKTIDE_UI_LANG", "en")
        .env("PACKTIDE_TEST_FORCE_INTERACTIVE", "1")
        .env("COLUMNS", "30")
        .output()
        .expect("run check-updates cancellation fixture");

    assert!(output.status.success());
    let rows = fs::read_to_string(fzf_input).expect("read update picker rows");
    assert!(
        rows.find("[Pacman").unwrap() < rows.find("[AUR").unwrap(),
        "rows={rows:?}"
    );
    assert!(
        rows.find("[AUR").unwrap() < rows.find("[Flatpak").unwrap(),
        "rows={rows:?}"
    );
    let args = fs::read_to_string(fzf_args).expect("read update picker args");
    assert!(args.contains("--track"));
    assert!(args.contains("--id-nth=2"));
    assert!(args.contains("reload-sync"));
    assert!(args.contains("--header"));
    assert!(args.contains("load:change-prompt(Updates available >)"));
    assert!(args.contains("Esc:exit"));
    assert!(args.contains("Enter:update system"));
    assert!(args.contains("Ctrl+R:refresh"));
}

#[test]
fn check_updates_fake_picker_accept_bridges_to_systide() {
    let fixture = Fixture::new();
    let cache = fixture.path().join("cache");
    let systide_args = fixture.path().join("systide.args");
    write_update_cache(&cache);
    fixture.write_executable("flatpak", "#!/bin/sh\nexit 0\n");
    let fzf_accept = fixture.path().join("fzf.accept");
    fixture.write_executable(
        "fzf",
        &format!(
            "#!/bin/sh\ncat >/dev/null\nprintf 'selected-row\\n' > '{}'\nexit 0\n",
            fzf_accept.display()
        ),
    );
    fixture.write_executable(
        "systide",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 0\n",
            systide_args.display()
        ),
    );
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");

    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["check-updates"])
        .env("PATH", path)
        .env("XDG_CACHE_HOME", cache)
        .env("PACKTIDE_TEST_SYSTIDE_BIN", fixture.path().join("systide"))
        .env("PACKTIDE_TEST_FORCE_INTERACTIVE", "1")
        .output()
        .expect("run check-updates accept fixture");

    assert!(
        output.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::read_to_string(systide_args).expect("read systide bridge args"),
        "--ui-lang\nauto\n--news-source\nofficial\n--count\n15\n"
    );
    assert_eq!(
        fs::read_to_string(fzf_accept).expect("read accepted picker marker"),
        "selected-row\n"
    );
}

#[test]
fn check_updates_picker_failure_is_not_reported_as_cancel() {
    let fixture = Fixture::new();
    let cache = fixture.path().join("cache");
    write_update_cache(&cache);
    fixture.write_executable("flatpak", "#!/bin/sh\nexit 0\n");
    fixture.write_executable("fzf", "#!/bin/sh\ncat >/dev/null\nexit 2\n");
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");

    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["check-updates"])
        .env("PATH", path)
        .env("XDG_CACHE_HOME", cache)
        .env("PACKTIDE_UI_LANG", "en")
        .env("PACKTIDE_TEST_FORCE_INTERACTIVE", "1")
        .output()
        .expect("run check-updates picker failure fixture");

    assert!(!output.status.success());
    assert!(!output.stderr.is_empty());
}

#[test]
fn install_catalog_failure_is_not_reported_as_no_selection() {
    let fixture = Fixture::new();
    let cache = fixture.path().join("cache");
    let aur_cache = cache.join("packtide/aur");
    fs::create_dir_all(&aur_cache).expect("create isolated AUR cache");
    fs::write(aur_cache.join("packages"), "").expect("write empty AUR cache");
    fixture.write_executable(
        "pacman",
        "#!/bin/sh\ncase \"$1:$2\" in\n  -Qq:) exit 0 ;;\n  --color=never:-Sl) printf 'catalog unavailable\\n' >&2; exit 42 ;;\n  *) exit 64 ;;\nesac\n",
    );
    fixture.write_executable("paru", "#!/bin/sh\nexit 0\n");
    fixture.write_executable("fzf", "#!/bin/sh\ncat >/dev/null\nexit 1\n");
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");

    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["install"])
        .env("PATH", path)
        .env("XDG_CACHE_HOME", cache)
        .env("PACKTIDE_UI_LANG", "en")
        .output()
        .expect("run install with failed package catalog");

    assert!(
        !output.status.success(),
        "source failure was hidden as cancellation"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("! Package source failed: pacman catalog exited"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("No packages selected"));
}

#[test]
fn empty_install_catalog_is_distinct_from_fzf_cancellation() {
    let fixture = Fixture::new();
    let cache = fixture.path().join("cache");
    let aur_cache = cache.join("packtide/aur");
    fs::create_dir_all(&aur_cache).expect("create isolated AUR cache");
    fs::write(aur_cache.join("packages"), "").expect("write empty AUR cache");
    let fzf_called = fixture.path().join("fzf.called");
    fixture.write_executable(
        "pacman",
        "#!/bin/sh\ncase \"$1:$2\" in\n  -Qq:) exit 0 ;;\n  --color=never:-Sl) exit 0 ;;\n  *) exit 64 ;;\nesac\n",
    );
    fixture.write_executable("paru", "#!/bin/sh\nexit 0\n");
    fixture.write_executable(
        "fzf",
        &format!(
            "#!/bin/sh\n: > '{}'\ncat >/dev/null\nexit 1\n",
            fzf_called.display()
        ),
    );
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");

    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["install"])
        .env("PATH", path)
        .env("XDG_CACHE_HOME", cache)
        .env("PACKTIDE_UI_LANG", "en")
        .output()
        .expect("run install with empty package catalog");

    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("No packages available for installation.")
    );
    assert!(
        fzf_called.exists(),
        "picker should still own empty-state rendering"
    );
}

#[test]
fn install_preview_covers_success_empty_and_failure_states() {
    let cases = [
        (
            "success",
            "printf 'Name : bash\\nVersion : 5.3-1\\n'",
            true,
            "Name",
        ),
        ("empty", "exit 0", true, "No details available."),
        (
            "failure",
            "printf 'lookup failed\\n' >&2; exit 17",
            true,
            "Preview failed",
        ),
    ];
    for (label, body, expected_success, expected_text) in cases {
        let fixture = Fixture::new();
        let invocations = fixture.path().join("paru.invocations");
        fixture.write_executable(
            "paru",
            &format!(
                "#!/bin/sh\nprintf 'invocation\\n' >> '{}'\n{body}\n",
                invocations.display()
            ),
        );
        let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
            .expect("build isolated PATH");
        let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
            .args(["__preview", "install", "core\tbash\t5.3-1"])
            .env("PATH", path)
            .env("PACKTIDE_UI_LANG", "en")
            .output()
            .unwrap_or_else(|error| panic!("run {label} preview: {error}"));

        assert_eq!(output.status.success(), expected_success, "case={label}");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(expected_text),
            "case={label} stdout={stdout}"
        );
        assert_eq!(
            fs::read_to_string(invocations)
                .expect("read fake helper invocation log")
                .lines()
                .count(),
            1,
            "case={label} should execute the details command exactly once"
        );
        if label == "success" {
            assert!(
                stdout.contains("\x1b[1;36mName\x1b[0m : bash"),
                "case={label} stdout={stdout}"
            );
        } else if label == "failure" {
            assert!(
                stdout.contains("lookup failed"),
                "case={label} should preserve details stderr: {stdout}"
            );
        }
    }
}

#[test]
fn preview_argument_errors_follow_selected_locale() {
    let english = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["__preview", "install"])
        .env("PACKTIDE_UI_LANG", "en")
        .output()
        .expect("run preview without a package row");
    assert!(!english.status.success());
    assert!(String::from_utf8_lossy(&english.stderr).contains("Preview requires a package name."));

    let invalid = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["__preview", "install", "core\t../escape\t5.3-1"])
        .env("PACKTIDE_UI_LANG", "en")
        .output()
        .expect("run preview with an invalid package row");
    assert!(!invalid.status.success());
    assert!(
        String::from_utf8_lossy(&invalid.stderr)
            .contains("Preview received an invalid package row.")
    );

    let chinese = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["__preview", "unknown", "core\tbash\t5.3-1"])
        .env("PACKTIDE_UI_LANG", "zh")
        .output()
        .expect("run preview with unknown kind");
    assert!(!chinese.status.success());
    assert!(String::from_utf8_lossy(&chinese.stderr).contains("未知的预览类型：unknown"));
}

#[test]
fn install_preview_preserves_colored_helper_sections_and_strips_osc_labels() {
    let fixture = Fixture::new();
    fixture.write_executable(
        "paru",
        "#!/bin/sh\nprintf '\\033[1;35mOptional Dependencies\\033[0m\\n\\033]8;;https://example.test\\007Name\\033]8;;\\007 : bash\\n'\n",
    );
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");
    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["__preview", "install", "core\tbash\t5.3-1"])
        .env("PATH", path)
        .env("PACKTIDE_UI_LANG", "en")
        .output()
        .expect("run helper section preview");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\x1b[1;35mOptional Dependencies\x1b[0m"));
    assert!(stdout.contains("\x1b[1;36mName\x1b[0m : bash"));
    assert!(!stdout.contains("\x1b]8;;https://example.test"));
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
fn downgrade_accept_preserves_preview_and_transaction_argv() {
    let fixture = Fixture::new();
    let fzf_args = fixture.path().join("downgrade.fzf.args");
    let transaction = fixture.path().join("downgrade.transaction");
    fixture.write_executable(
        "pacman",
        "#!/bin/sh\ncase \"$2\" in\n  -Sl) printf 'core bash 5.3-1\\n' ;;\n  -Q) printf 'bash 5.4-1\\n' ;;\n  *) exit 64 ;;\nesac\n",
    );
    fixture.write_executable("paru", "#!/bin/sh\ncase \"$1:$2\" in --color=always:-Qi) printf 'Name : bash\\nVersion : 5.4-1\\n' ;; *) exit 0 ;; esac\n");
    fixture.write_executable(
        "downgrade",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 0\n",
            transaction.display()
        ),
    );
    fixture.write_executable(
        "fzf",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\ncat >/dev/null\nprintf 'core bash 5.4-1\\n'\nexit 0\n",
            fzf_args.display()
        ),
    );
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");
    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["downgrade"])
        .env("PATH", path)
        .env("PACKTIDE_UI_LANG", "en")
        .env(
            "PACKTIDE_TEST_DOWNGRADE_BIN",
            fixture.path().join("downgrade"),
        )
        .output()
        .expect("run downgrade accept fixture");

    assert!(output.status.success());
    let args = fs::read_to_string(fzf_args).expect("read downgrade fzf args");
    assert!(args.contains("--preview"));
    assert!(args.contains("__preview downgrade"));
    assert!(args.contains("--no-wrap"));
    assert!(args.contains("--ellipsis=..."));
    assert_eq!(
        fs::read_to_string(transaction).expect("read downgrade transaction"),
        "downgrade\nbash\n"
    );
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
        .env("COLUMNS", "30")
        .output()
        .expect("run localized downgrade picker");

    assert!(output.status.success());
    let args = fs::read_to_string(fzf_args).expect("read fzf arguments");
    assert!(args.contains("PACKTIDE · Downgrade Packages"));
    assert!(args.contains("Packages to downgrade >"));
    assert!(args.contains("__preview downgrade"));
    assert!(args.contains("Tab:select"));
    assert!(args.contains("Enter:downgrade"));
    assert!(args.contains("Esc:exit"));
}

#[test]
fn downgrade_preview_runs_helper_with_color_and_renders_metadata() {
    let fixture = Fixture::new();
    let helper_args = fixture.path().join("paru.argv");
    fixture.write_executable(
        "paru",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nprintf '\\033[1mName\\033[0m : bash\\nVersion : 5.3-1\\n'\n",
            helper_args.display()
        ),
    );
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");
    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["__preview", "downgrade", "core\tbash\t5.3-1"])
        .env("PATH", path)
        .env("PACKTIDE_UI_LANG", "en")
        .output()
        .expect("run downgrade preview");

    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(helper_args).expect("read helper argv"),
        "--color=always\n-Qi\nbash\n"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\x1b[1;36mName\x1b[0m : bash"));
    assert!(stdout.contains("\x1b[1;36mVersion\x1b[0m : 5.3-1"));
}

#[test]
fn preview_details_use_installed_query_only_for_remove_and_downgrade() {
    let fixture = Fixture::new();
    let pacman_argv = fixture.path().join("pacman.preview.argv");
    let paru_argv = fixture.path().join("paru.preview.argv");
    fixture.write_executable(
        "pacman",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\ncase \"$1:$2\" in\n  --color=always:-Qi) printf 'Name : bash\\nVersion : 5.3-1\\n' ;;\n  *) exit 64 ;;\nesac\n",
            pacman_argv.display()
        ),
    );
    fixture.write_executable(
        "paru",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\ncase \"$1:$2\" in\n  --color=always:-Qi|--color=always:-Si) printf 'Name : bash\\nVersion : 5.3-1\\n' ;;\n  *) exit 64 ;;\nesac\n",
            paru_argv.display()
        ),
    );
    fixture.write_executable(
        "sudo",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 64\n",
            fixture.path().join("sudo.argv").display()
        ),
    );
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");
    let run_preview = |kind: &str, row: &str| {
        let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
            .args(["__preview", kind, row])
            .env("PATH", &path)
            .env("PACKTIDE_UI_LANG", "en")
            .output()
            .expect("run package preview");
        assert!(
            output.status.success(),
            "{kind} preview failed; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };

    run_preview("remove", "core\tbash\t5.3-1");
    assert_eq!(
        fs::read_to_string(&pacman_argv).expect("read Pacman remove preview argv"),
        "--color=always\n-Qi\nbash\n"
    );

    run_preview("remove", "aur\tbash\t5.3-1");
    assert_eq!(
        fs::read_to_string(&paru_argv).expect("read AUR remove preview argv"),
        "--color=always\n-Qi\nbash\n"
    );

    run_preview("install", "core\tbash\t5.3-1");
    assert_eq!(
        fs::read_to_string(&paru_argv).expect("read install preview argv"),
        "--color=always\n-Si\ncore/bash\n"
    );

    run_preview("downgrade", "core\tbash\t5.3-1");
    assert_eq!(
        fs::read_to_string(&paru_argv).expect("read downgrade preview argv"),
        "--color=always\n-Qi\nbash\n"
    );
    assert!(!fixture.path().join("sudo.argv").exists());
}

#[test]
fn mirror_update_missing_reflector_names_the_affected_operation() {
    assert_missing_required_command("mirror-update", "reflector", "the mirror list update");
}

#[test]
fn sysup_compatibility_bridge_reports_systide_dependency() {
    let fixture = Fixture::new();
    let output = Command::new(env!("CARGO_BIN_EXE_packtide"))
        .args(["sysup", "--ui-lang", "en"])
        .env("PATH", fixture.path())
        .output()
        .expect("run packtide sysup with no package helpers");
    let error = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(
        error.contains("No AUR helper found")
            || error.contains("required command 'systide'")
            || error.contains("Required package manager command is unavailable")
    );
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
