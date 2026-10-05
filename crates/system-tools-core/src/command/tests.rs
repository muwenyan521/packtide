use super::{
    PRIVILEGED_COMMAND_PATH, PrivilegeRunner, capture_plan_with_runner, debug_timing_line,
    debug_timings_enabled, run_command_plan, run_status_path_timeout,
};
use crate::{CommandPlan, CommandPrivilege};
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
fn bounded_status_runner_kills_a_stalled_optional_update() {
    let fixture = std::env::temp_dir().join(format!(
        "system-tools-core-timeout-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&fixture).expect("create timeout fixture");
    let program = fixture.join("slow");
    write_executable(&program, "#!/bin/sh\nsleep 2\n");
    let error = run_status_path_timeout(&program, &[] as &[&str], Duration::from_millis(20))
        .expect_err("slow command must time out");
    assert!(error.to_string().contains("timed out"));
    fs::remove_dir_all(fixture).expect("remove timeout fixture");
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
        "system-tools-core-privilege-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos()
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

#[test]
fn command_plan_runner_preserves_absolute_argv_and_environment_policy() {
    let fixture = std::env::temp_dir().join(format!(
        "system-tools-core-command-plan-runner-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&fixture);
    fs::create_dir_all(&fixture).expect("create command plan fixture");
    let user_log = fixture.join("user.log");
    let user_program = fixture.join("fake-tool");
    write_executable(
        &user_program,
        &format!(
            "#!/bin/sh\nprintf 'LC_ALL=%s\nLD_PRELOAD=%s\n' \"${{LC_ALL-<unset>}}\" \"${{LD_PRELOAD-<unset>}}\" > '{}'\nprintf '%s\n' \"$@\" >> '{}'\n",
            user_log.display(),
            user_log.display()
        ),
    );
    let user_plan = CommandPlan::new(user_program.clone())
        .arg("--fixed")
        .arg("value")
        .with_env_remove("LD_PRELOAD")
        .with_locale("C");
    run_command_plan(&user_plan).expect("run user command plan");
    assert_eq!(
        fs::read_to_string(&user_log).expect("read user command log"),
        "LC_ALL=C\nLD_PRELOAD=<unset>\n--fixed\nvalue\n"
    );
    assert!(user_plan.program.is_absolute());

    let trusted = fixture.join("trusted");
    let elevated_log = fixture.join("elevated.log");
    fs::create_dir(&trusted).expect("create trusted sudo fixture");
    write_executable(
        &trusted.join("sudo"),
        &format!(
            "#!/bin/sh\nprintf 'PATH=%s\\nLD_PRELOAD=%s\\nLD_LIBRARY_PATH=%s\\n' \"$PATH\" \"${{LD_PRELOAD-<unset>}}\" \"${{LD_LIBRARY_PATH-<unset>}}\" > '{}'\nprintf '%s\\n' \"$@\" >> '{}'\n",
            elevated_log.display(),
            elevated_log.display()
        ),
    );
    let runner = PrivilegeRunner::from_search_path(trusted.as_os_str()).expect("trusted sudo");
    let elevated_plan = CommandPlan::new(user_program)
        .arg("--elevated")
        .with_privilege(CommandPrivilege::Elevated)
        .with_locale("C");
    runner
        .run_plan(&elevated_plan)
        .expect("run elevated command plan");
    assert_eq!(
        fs::read_to_string(elevated_log).expect("read elevated command log"),
        format!(
            "PATH={PRIVILEGED_COMMAND_PATH}\nLD_PRELOAD=<unset>\nLD_LIBRARY_PATH=<unset>\n{}\n--elevated\n",
            elevated_plan.program.display()
        )
    );
    fs::remove_dir_all(fixture).expect("remove command plan fixture");
}

#[test]
fn captured_elevated_plan_uses_sudo_and_keeps_environment_argv_and_failure_output() {
    // Given a fake sudo that records its environment and structured arguments.
    let fixture = std::env::temp_dir().join(format!(
        "system-tools-core-capture-plan-{}",
        std::process::id()
    ));
    fs::create_dir_all(&fixture).expect("create capture fixture");
    write_executable(
        &fixture.join("sudo"),
        "#!/bin/sh\nprintf '%s\\n' \"$PATH\" \"${LC_ALL-<unset>}\" \"${HOME-<unset>}\" \"${PYTHONPATH-<unset>}\" \"$@\"\nprintf 'sudo failure\\n' >&2\nexit 23\n",
    );
    let plan = CommandPlan::new(fixture.join("must-not-run-directly"))
        .arg("argument with spaces")
        .with_env_remove("HOME")
        .with_locale("C")
        .with_privilege(CommandPrivilege::Elevated);

    // When the capture dispatcher executes an elevated plan.
    let output = capture_plan_with_runner(&plan, || {
        PrivilegeRunner::from_search_path(fixture.as_os_str())
    })
    .expect("capture fake sudo failure");

    // Then sudo is used, the policy is applied, and failure output is retained.
    assert_eq!(output.status.code(), Some(23));
    assert_eq!(
        output.stdout,
        format!(
            "{PRIVILEGED_COMMAND_PATH}\nC\n<unset>\n<unset>\n{}\nargument with spaces\n",
            plan.program.display()
        )
    );
    assert_eq!(output.stderr, "sudo failure\n");
    fs::remove_dir_all(fixture).expect("remove capture fixture");
}

#[test]
fn captured_elevated_plan_does_not_fall_back_when_privilege_runner_is_unavailable() {
    // Given a command that would succeed if executed without elevation.
    let plan = CommandPlan::new("/bin/true".into()).with_privilege(CommandPrivilege::Elevated);

    // When trusted sudo cannot be resolved.
    let result = capture_plan_with_runner(&plan, || anyhow::bail!("trusted sudo unavailable"));

    // Then the resolution error propagates instead of executing directly.
    assert_eq!(
        result.err().expect("must fail").to_string(),
        "trusted sudo unavailable"
    );
}

#[test]
fn keyring_plan_uses_absolute_pacman_locale_scrubbing_and_trusted_sudo() {
    let fixture =
        std::env::temp_dir().join(format!("system-tools-core-keyring-{}", std::process::id()));
    let _ = fs::remove_dir_all(&fixture);
    let bin = fixture.join("bin");
    fs::create_dir_all(&bin).expect("create fixture bin");
    let log = fixture.join("sudo.log");
    write_executable(&bin.join("pacman"), "#!/bin/sh\nexit 0\n");
    write_executable(
        &bin.join("sudo"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \\\n  \\\"PATH=$PATH\\\" \\\n  \\\"LC_ALL=${{LC_ALL-<unset>}}\\\" \\\n  \\\"LD_PRELOAD=${{LD_PRELOAD-<unset>}}\\\" > '{}'\nprintf '%s\\n' \\\"$@\\\" >> '{}'\n",
            log.display(),
            log.display()
        ),
    );
    let resolver = crate::ExecutableResolver::from_path(Some(bin.as_os_str()));
    let plan =
        crate::package_keyring_plan(&["archlinux-keyring"], &resolver).expect("build keyring plan");
    assert_eq!(plan.program, bin.join("pacman"));
    assert_eq!(plan.locale.as_deref(), Some(std::ffi::OsStr::new("C")));
    let runner = PrivilegeRunner::from_search_path(bin.as_os_str()).expect("resolve trusted sudo");
    runner.run_plan(&plan).expect("run fake sudo");
    let captured = fs::read_to_string(&log).expect("read sudo capture");
    assert!(captured.contains("PATH=/usr/bin:/bin:/usr/sbin:/sbin"));
    assert!(captured.contains("LC_ALL=C"));
    assert!(captured.contains("LD_PRELOAD=<unset>"));
    assert!(captured.contains(&format!("{}\n", bin.join("pacman").display())));
    assert!(captured.contains("-Sy"), "captured={captured:?}");
    assert!(captured.contains("--needed"));
    assert!(captured.contains("--noconfirm"));
    assert!(captured.contains("archlinux-keyring"));
    fs::remove_dir_all(fixture).expect("remove fixture");
}
