use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

#[test]
fn optional_update_list_preserves_providers_and_reports_failures() {
    let fixture = Fixture::new();
    fixture.write_executable(
        "snap",
        "#!/bin/sh\nprintf 'Name Version Rev Publisher Notes\\nhello-world 2 2 acme -\\n'\n",
    );
    fixture.write_executable("brew", "#!/bin/sh\nprintf '%s\\n' '{\"formulae\":[{\"name\":\"hello-world\",\"installed_versions\":[\"1\"],\"latest_version\":\"2\"}]}'\n");
    fixture.write_executable("nix", "#!/bin/sh\ncase \"$*\" in\n *profile*) printf '%s\\n' '{\"elements\":{\"hello-world\":{\"attrPath\":\"hello\",\"originalUrl\":\"flake:nixpkgs\",\"storePaths\":[\"/nix/store/old-hello\"],\"active\":true}}}' ;;\n *) printf '%s\\n' '\"/nix/store/new-hello\"' ;;\nesac\n");
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_systide"))
            .arg("--list-data")
            .env("PATH", fixture.path())
            .output()
            .expect("run list-data")
    };
    let output = invoke();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows = String::from_utf8_lossy(&output.stdout);
    for source in ["[snap]", "[brew]", "[nix]"] {
        assert!(rows.contains(source), "missing {source}: {rows}");
    }
    assert!(rows.contains("hello-world 1 -> 2"));
    fixture.write_executable(
        "snap",
        "#!/bin/sh\nprintf 'snap service unavailable\\n' >&2\nexit 1\n",
    );
    let failed_provider = invoke();
    assert!(failed_provider.status.success());
    assert!(String::from_utf8_lossy(&failed_provider.stderr).contains("update diagnostics: snap:"));
    assert!(String::from_utf8_lossy(&failed_provider.stdout).contains("[brew]"));
    println!(
        "PROVIDER ROWS:\n{rows}\nFAILURE DIAGNOSTICS:\n{}",
        String::from_utf8_lossy(&failed_provider.stderr)
    );
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("systide-cli-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).expect("create isolated PATH fixture");
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
                "cannot remove systide CLI fixture {}: {error}",
                self.0.display()
            );
        }
    }
}

#[test]
fn list_missing_fzf_names_the_affected_picker() {
    let fixture = Fixture::new();
    let output = Command::new(env!("CARGO_BIN_EXE_systide"))
        .args(["--list", "--ui-lang", "en"])
        .env("PATH", fixture.path())
        .output()
        .expect("run systide with an isolated empty PATH");

    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("Required package manager command is unavailable"));
    assert!(error.contains("Update catalog"));
}

#[test]
fn list_cancel_preserves_source_order_and_localized_layout() {
    let fixture = Fixture::new();
    let rows = fixture.path().join("rows");
    let args = fixture.path().join("args");
    fixture.write_executable("checkupdates", "#!/bin/sh\nprintf 'bash 5.3 -> 5.4\\n'\n");
    fixture.write_executable("paru", "#!/bin/sh\nprintf 'aur-tool 1.0 -> 1.1\\n'\n");
    fixture.write_executable("flatpak", "#!/bin/sh\nprintf 'org.example.App 2.0\\n'\n");
    fixture.write_executable(
        "fzf",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\ncat > '{}'\nexit 1\n",
            args.display(),
            rows.display()
        ),
    );
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");

    let output = Command::new(env!("CARGO_BIN_EXE_systide"))
        .args(["--list", "--ui-lang", "zh"])
        .env("PATH", path)
        .output()
        .expect("run systide list cancellation fixture");

    assert!(output.status.success());
    let input = fs::read_to_string(rows).expect("read systide list rows");
    assert!(input.find("[官方源").unwrap() < input.find("[AUR").unwrap());
    assert!(input.find("[AUR").unwrap() < input.find("[Flatpak").unwrap());
    let fzf_args = fs::read_to_string(args).expect("read systide fzf args");
    assert!(fzf_args.contains("--track"));
    assert!(fzf_args.contains("--id-nth=2"));
    assert!(fzf_args.contains("reload-sync"));
    assert!(fzf_args.contains("load:change-prompt(待更新项目 >)"));
    assert!(fzf_args.contains("--header"));
    assert!(fzf_args.contains("--prompt"));
    assert!(fzf_args.contains("待更新项目"));
}

#[test]
fn list_picker_failure_is_not_reported_as_cancellation() {
    let fixture = Fixture::new();
    fixture.write_executable("checkupdates", "#!/bin/sh\nprintf 'bash 5.3 -> 5.4\\n'\n");
    fixture.write_executable("fzf", "#!/bin/sh\ncat >/dev/null\nexit 2\n");
    let path = std::env::join_paths([fixture.path(), Path::new("/usr/bin"), Path::new("/bin")])
        .expect("build isolated PATH");

    let output = Command::new(env!("CARGO_BIN_EXE_systide"))
        .args(["--list", "--ui-lang", "en"])
        .env("PATH", path)
        .output()
        .expect("run systide list failure fixture");

    assert!(!output.status.success());
    assert!(!output.stderr.is_empty());
}
