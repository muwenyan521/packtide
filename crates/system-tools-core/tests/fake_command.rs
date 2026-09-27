use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use system_tools_core::{run_capture, run_status};

fn fixture_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("system-tools-core-{name}-{}", std::process::id()))
}

#[test]
fn fake_command_records_argv_stdout_stderr_and_exit_status() {
    let directory = fixture_dir("fake-command");
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("create fixture directory");
    let script = directory.join("fake-tool");
    let log = directory.join("argv.log");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nprintf 'stdout\\n'\nprintf 'stderr\\n' >&2\nexit 7\n",
            log.display()
        ),
    )
    .expect("write fake command");
    let mut permissions = fs::metadata(&script)
        .expect("stat fake command")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script, permissions).expect("chmod fake command");

    let output = run_capture(
        script.to_str().expect("fixture path is UTF-8"),
        &["--alpha", "value"],
        true,
    )
    .expect("run fake command");
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, "stdout\n");
    assert_eq!(output.stderr, "stderr\n");
    assert_eq!(
        fs::read_to_string(log).expect("read argv log"),
        "--alpha\nvalue\n"
    );

    let error = run_status(script.to_str().expect("fixture path is UTF-8"), &["--beta"]);
    assert!(error.is_err());
    fs::remove_dir_all(directory).expect("remove fixture directory");
}
