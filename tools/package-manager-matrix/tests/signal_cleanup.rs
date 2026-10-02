#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
fn sigterm_reaps_curl_and_removes_cloud_temp() {
    let root = std::env::temp_dir().join(format!("pm-matrix-signal-test-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let curl = root.join("curl");
    fs::write(&curl, "#!/bin/sh\nexec sleep 60\n").unwrap();
    fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();
    let tmp = root.join("tmp");
    fs::create_dir_all(&tmp).unwrap();
    let path = std::env::var("PATH").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_package-manager-matrix"))
        .arg("verify-cloud-image")
        .env("PATH", format!("{}:{}", root.display(), path))
        .env("TMPDIR", &tmp)
        .current_dir(format!("{}/../..", env!("CARGO_MANIFEST_DIR")))
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_millis(250));
    unsafe {
        libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
    }
    let status = child.wait().unwrap();
    assert!(!status.success());
    let leftovers = fs::read_dir(&tmp).unwrap().collect::<Vec<_>>();
    assert!(leftovers.is_empty(), "cloud temp remains: {:?}", leftovers);
    let _ = fs::remove_dir_all(root);
}
