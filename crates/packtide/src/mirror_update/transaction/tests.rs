use super::{MirrorBackup, PrivilegedCommandRunner};
use anyhow::{Result, bail};
use std::cell::Cell;
use std::cell::RefCell;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

struct FakePrivilegedRunner {
    calls: RefCell<Vec<Vec<OsString>>>,
    fail_install: bool,
    fail_publish: bool,
    fail_restore: bool,
    panic_publish: Cell<bool>,
}

impl PrivilegedCommandRunner for FakePrivilegedRunner {
    fn run(&self, args: &[OsString]) -> Result<()> {
        self.calls.borrow_mut().push(args.to_vec());
        let command = args.first().map(|arg| arg.to_string_lossy());
        match command.as_deref() {
            Some("install") => {
                let source = Path::new(&args[3]);
                let staging = Path::new(&args[4]);
                if self.fail_restore
                    && source
                        .file_name()
                        .is_some_and(|name| name == "mirrorlist.bak")
                {
                    bail!("fake restore staging failed")
                }
                fs::copy(source, staging)?;
                if self.fail_install {
                    bail!("fake staging install failed")
                }
                Ok(())
            }
            Some("mv") => {
                fs::rename(Path::new(&args[2]), Path::new(&args[3]))?;
                if self.panic_publish.replace(false) {
                    panic!("fake mirror publisher panic");
                }
                if self.fail_publish {
                    bail!("fake mirror publication failed after rename")
                }
                Ok(())
            }
            Some("rm") => {
                let target = Path::new(&args[2]);
                if target.exists() {
                    fs::remove_file(target)?;
                }
                Ok(())
            }
            _ => bail!("unexpected privileged command"),
        }
    }
}

fn fixture(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "packtide-mirror-transaction-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create fixture");
    path
}

#[test]
fn staging_failure_leaves_original_mirrorlist_unchanged() {
    // Given an existing mirrorlist and a staging command that fails
    let directory = fixture("restore-original");
    let target = directory.join("mirrorlist");
    fs::write(&target, "original").expect("write original mirrorlist");
    let backup = MirrorBackup::new(&target, &directory).expect("create mirror backup");
    let runner = FakePrivilegedRunner {
        calls: RefCell::new(Vec::new()),
        fail_install: true,
        fail_publish: false,
        fail_restore: false,
        panic_publish: Cell::new(false),
    };

    // When publishing the generated list fails
    let result = backup.install("generated", &runner);

    // Then the original content remains and the staged file is cleaned
    assert!(result.is_err());
    assert_eq!(
        fs::read_to_string(&target).expect("read restored mirrorlist"),
        "original"
    );
    let calls = runner.calls.borrow();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0][0], "install");
    assert_ne!(Path::new(&calls[0][4]), target);
    assert_eq!(calls[1][0], "rm");
    fs::remove_dir_all(directory).expect("remove fixture");
}

#[test]
fn install_failure_removes_target_when_no_original_existed() {
    // Given no original mirrorlist and an atomic publication that fails after rename
    let directory = fixture("remove-new-target");
    let target = directory.join("mirrorlist");
    let backup = MirrorBackup::new(&target, &directory).expect("create mirror backup");
    let runner = FakePrivilegedRunner {
        calls: RefCell::new(Vec::new()),
        fail_install: false,
        fail_publish: true,
        fail_restore: false,
        panic_publish: Cell::new(false),
    };

    // When publishing the generated list fails
    let result = backup.install("generated", &runner);

    // Then the newly created partial target is removed
    assert!(result.is_err());
    assert!(!target.exists());
    let calls = runner.calls.borrow();
    assert_eq!(calls[2][0], "rm");
    assert_eq!(Path::new(&calls[2][2]), target);
    fs::remove_dir_all(directory).expect("remove fixture");
}

#[test]
fn install_success_commits_new_mirrorlist_and_keeps_backup() {
    // Given an existing mirrorlist and a successful privileged installer
    let directory = fixture("commit");
    let target = directory.join("mirrorlist");
    fs::write(&target, "original").expect("write original mirrorlist");
    let backup = MirrorBackup::new(&target, &directory).expect("create mirror backup");
    let runner = FakePrivilegedRunner {
        calls: RefCell::new(Vec::new()),
        fail_install: false,
        fail_publish: false,
        fail_restore: false,
        panic_publish: Cell::new(false),
    };

    // When the generated list is installed
    backup
        .install("generated", &runner)
        .expect("install mirrorlist");

    // Then the target changes and the user-visible backup remains intact
    assert_eq!(
        fs::read_to_string(&target).expect("read new mirrorlist"),
        "generated"
    );
    assert_eq!(
        fs::read_to_string(backup.backup_path()).expect("read backup"),
        "original"
    );
    let calls = runner.calls.borrow();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0][0], "install");
    assert_eq!(calls[1][0], "mv");
    assert_eq!(Path::new(&calls[0][4]).parent(), target.parent());
    assert_eq!(calls[1][2], calls[0][4]);
    assert_eq!(Path::new(&calls[1][3]), target);
    fs::remove_dir_all(directory).expect("remove fixture");
}

#[test]
fn rollback_guard_restores_original_during_unwind() {
    // Given an existing mirrorlist and a publisher that panics after rename
    let directory = fixture("unwind");
    let target = directory.join("mirrorlist");
    fs::write(&target, "original").expect("write original mirrorlist");
    let backup = MirrorBackup::new(&target, &directory).expect("create mirror backup");
    let runner = FakePrivilegedRunner {
        calls: RefCell::new(Vec::new()),
        fail_install: false,
        fail_publish: false,
        fail_restore: false,
        panic_publish: Cell::new(true),
    };

    // When installation unwinds
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = backup.install("generated", &runner);
    }));

    // Then Drop restores the original target
    assert!(result.is_err());
    assert_eq!(
        fs::read_to_string(&target).expect("read restored mirrorlist"),
        "original"
    );
    let calls = runner.calls.borrow();
    assert_eq!(calls[2][0], "install");
    assert_eq!(Path::new(&calls[2][4]).parent(), target.parent());
    assert_eq!(calls[3][0], "mv");
    assert_eq!(Path::new(&calls[3][3]), target);
    fs::remove_dir_all(directory).expect("remove fixture");
}

#[test]
fn rollback_failure_is_reported_with_install_failure() {
    // Given an installer and rollback command that both fail
    let directory = fixture("rollback-failure");
    let target = directory.join("mirrorlist");
    fs::write(&target, "original").expect("write original mirrorlist");
    let backup = MirrorBackup::new(&target, &directory).expect("create mirror backup");
    let runner = FakePrivilegedRunner {
        calls: RefCell::new(Vec::new()),
        fail_install: false,
        fail_publish: true,
        fail_restore: true,
        panic_publish: Cell::new(false),
    };

    // When publishing and restoring fail
    let result = backup.install("generated", &runner);

    // Then the returned error reports rollback failure
    let error = result.expect_err("mirror install must fail");
    assert!(error.to_string().contains("mirrorlist rollback failed"));
    fs::remove_dir_all(directory).expect("remove fixture");
}
