use super::{PRIVILEGED_COMMAND_PATH, PrivilegeRunner, debug_timing_line, debug_timings_enabled};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::process::Command;
use std::process::ExitStatus;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

fn write_executable(path: &std::path::Path, contents: &str) {
    fs::write(path, contents).expect("write command fixture");
    let mut permissions = fs::metadata(path)
        .expect("stat command fixture")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("make command fixture executable");
}

#[test]
fn debug_timing_switch_accepts_explicit_truthy_values_only() {
    assert!(debug_timings_enabled(Some("1")));
    assert!(debug_timings_enabled(Some("true")));
    assert!(debug_timings_enabled(Some("yes")));
    assert!(!debug_timings_enabled(None));
    assert!(!debug_timings_enabled(Some("0")));
    assert!(!debug_timings_enabled(Some("TRUE")));
}

#[test]
fn debug_timing_line_contains_only_command_timing_fields() {
    let line = debug_timing_line(
        std::ffi::OsStr::new("fake-tool"),
        Duration::from_millis(17),
        ExitStatus::from_raw(0),
    );
    assert_eq!(
        line,
        "command_timing program=fake-tool elapsed_ms=17 status=exit status: 0"
    );
}

#[test]
fn privilege_runner_scrubs_inherited_loader_and_language_environment() {
    const CHILD_SEARCH_PATH: &str = "SYSTEM_TOOLS_CORE_PRIVILEGE_TEST_SEARCH_PATH";
    if let Some(search_path) = std::env::var_os(CHILD_SEARCH_PATH) {
        PrivilegeRunner::from_search_path(&search_path)
            .expect("resolve fake sudo in child")
            .run(&["pacman", "-Su"])
            .expect("run fake sudo in child");
        return;
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_nanos();
    let fixture = std::env::temp_dir().join(format!(
        "system-tools-core-privilege-env-{}-{nonce}",
        std::process::id()
    ));
    let trusted = fixture.join("trusted");
    let log = fixture.join("sudo-environment.log");
    let home = fixture.join("home");
    fs::create_dir_all(&trusted).expect("create trusted fixture");
    fs::create_dir(&home).expect("create controlled HOME");
    write_executable(
        &trusted.join("sudo"),
        r#"#!/bin/sh
printf '%s\n' \
  "PATH=$PATH" \
  "HOME=${HOME-<unset>}" \
  "LD_PRELOAD=${LD_PRELOAD-<unset>}" \
  "LD_LIBRARY_PATH=${LD_LIBRARY_PATH-<unset>}" \
  "PYTHONPATH=${PYTHONPATH-<unset>}" \
  "PYTHONHOME=${PYTHONHOME-<unset>}" \
  "RUBYLIB=${RUBYLIB-<unset>}" \
  "PERL5LIB=${PERL5LIB-<unset>}" \
  "BASH_ENV=${BASH_ENV-<unset>}" \
  "ENV=${ENV-<unset>}" > "$FAKE_LOG"
printf '%s\n' "$@" >> "$FAKE_LOG"
exit 0
"#,
    );

    let output = Command::new(std::env::current_exe().expect("locate test binary"))
        .args([
            "--exact",
            "command::tests::privilege_runner_scrubs_inherited_loader_and_language_environment",
            "--nocapture",
        ])
        .env(CHILD_SEARCH_PATH, &trusted)
        .env("PATH", fixture.join("hostile"))
        .env("FAKE_LOG", &log)
        .env("HOME", &home)
        .env("LD_PRELOAD", "")
        .env("LD_LIBRARY_PATH", &fixture)
        .env("PYTHONPATH", "untrusted-python-path")
        .env("PYTHONHOME", "untrusted-python-home")
        .env("RUBYLIB", "untrusted-ruby-path")
        .env("PERL5LIB", "untrusted-perl-path")
        .env("BASH_ENV", "untrusted-bash-env")
        .env("ENV", "untrusted-shell-env")
        .output()
        .expect("run isolated privilege test child");
    assert!(
        output.status.success(),
        "child test failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let expected = format!(
        "PATH={PRIVILEGED_COMMAND_PATH}\nHOME={}\nLD_PRELOAD=<unset>\nLD_LIBRARY_PATH=<unset>\nPYTHONPATH=<unset>\nPYTHONHOME=<unset>\nRUBYLIB=<unset>\nPERL5LIB=<unset>\nBASH_ENV=<unset>\nENV=<unset>\npacman\n-Su\n",
        home.display()
    );
    assert_eq!(
        fs::read_to_string(log).expect("read fake sudo environment"),
        expected
    );
    fs::remove_dir_all(fixture).expect("remove privilege environment fixture");
}

#[test]
fn privilege_runner_uses_trusted_sudo_and_fixed_path() {
    // Given trusted and hostile directories containing a sudo executable
    let fixture = std::env::temp_dir().join(format!(
        "system-tools-core-privilege-{}",
        std::process::id()
    ));
    let trusted = fixture.join("trusted");
    let hostile = fixture.join("hostile");
    let _ = fs::remove_dir_all(&fixture);
    fs::create_dir_all(&trusted).expect("create trusted fixture");
    fs::create_dir_all(&hostile).expect("create hostile fixture");
    let trusted_log = fixture.join("trusted.log");
    let hostile_log = fixture.join("hostile.log");
    write_executable(
        &trusted.join("sudo"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$PATH\" \"$@\" > '{}'\nexit 0\n",
            trusted_log.display()
        ),
    );
    write_executable(
        &hostile.join("sudo"),
        &format!(
            "#!/bin/sh\nprintf called > '{}'\nexit 0\n",
            hostile_log.display()
        ),
    );

    // When the runner searches the trusted path and receives a transaction argv
    let runner =
        PrivilegeRunner::from_search_path(trusted.as_os_str()).expect("resolve trusted sudo");
    runner.run(&["pacman", "-Su"]).expect("run fake sudo");

    // Then it chooses the trusted binary, fixes PATH, and preserves argv
    let expected = format!("{PRIVILEGED_COMMAND_PATH}\npacman\n-Su\n");
    assert_eq!(
        fs::read_to_string(trusted_log).expect("trusted invocation"),
        expected
    );
    assert!(!hostile_log.exists());
    fs::remove_dir_all(fixture).expect("remove privilege fixture");
}

#[test]
fn privilege_runner_propagates_sudo_failure() {
    // Given a trusted fake sudo that exits unsuccessfully
    let fixture = std::env::temp_dir().join(format!(
        "system-tools-core-privilege-failure-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&fixture);
    fs::create_dir_all(&fixture).expect("create failure fixture");
    write_executable(&fixture.join("sudo"), "#!/bin/sh\nexit 23\n");

    // When a privileged command is run
    let runner = PrivilegeRunner::from_search_path(fixture.as_os_str()).expect("resolve fake sudo");
    let result = runner.run(&["pacman", "-Su"]);

    // Then the nonzero sudo result remains an error
    assert!(result.is_err());
    fs::remove_dir_all(fixture).expect("remove failure fixture");
}
