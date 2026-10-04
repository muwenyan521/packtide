use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn update_provider_failures_and_versions_are_observable() {
    let path =
        std::env::temp_dir().join(format!("packtide-update-contract-{}", std::process::id()));
    fs::create_dir_all(&path).expect("fixture directory");
    let write = |name: &str, body: &str| {
        let executable = path.join(name);
        fs::write(&executable, format!("#!/bin/sh\n{body}\n")).expect("fixture script");
        fs::set_permissions(executable, fs::Permissions::from_mode(0o755))
            .expect("fixture permissions");
    };
    for name in ["pacman", "apt-cache", "dpkg-query"] {
        write(name, "exit 0");
    }
    write("checkupdates", "printf 'native-contract 1 -> 2\\n'");
    write(
        "apt-get",
        "printf 'Inst native-contract [1] (2 stable [amd64])\\n'",
    );
    write(
        "brew",
        "printf '%s\\n' '{\"formulae\":[{\"name\":\"optional-contract\",\"installed_versions\":[\"1\"],\"latest_version\":\"2\"}]}'",
    );
    write("snap", "printf 'snap service unavailable\\n' >&2; exit 1");
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_packtide"))
            .args(["check-updates", "--refresh"])
            .env("PACKTIDE_UPDATE_LIST_ONLY", "1")
            .env("XDG_CACHE_HOME", path.join("cache"))
            .env("PATH", &path)
            .output()
            .expect("run packtide")
    };
    let success = invoke();
    assert!(
        success.status.success(),
        "{}",
        String::from_utf8_lossy(&success.stderr)
    );
    let rows = String::from_utf8_lossy(&success.stdout);
    assert!(rows.contains("native-contract 1 -> 2"));
    assert!(rows.contains("optional-contract 1 -> 2"));
    assert!(String::from_utf8_lossy(&success.stderr).contains("update diagnostics: snap:"));
    write("checkupdates", "printf 'native read failed\\n' >&2; exit 1");
    write("apt-get", "printf 'native read failed\\n' >&2; exit 1");
    let failure = invoke();
    assert!(!failure.status.success());
    assert!(String::from_utf8_lossy(&failure.stderr).contains("native read failed"));
    println!(
        "SUCCESS ROWS:\n{rows}\nSUCCESS DIAGNOSTICS:\n{}\nFAILURE STATUS: {}\nFAILURE DIAGNOSTICS:\n{}",
        String::from_utf8_lossy(&success.stderr),
        failure.status,
        String::from_utf8_lossy(&failure.stderr)
    );
    fs::remove_dir_all(path).expect("remove fixture");
}
