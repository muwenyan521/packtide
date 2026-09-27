use std::fs;
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
        let path = std::env::temp_dir().join(format!("systide-cli-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).expect("create isolated PATH fixture");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
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
    assert!(
        error.contains("required command 'fzf' is unavailable for the system update list picker")
    );
    assert!(error.contains("install it and retry"));
}
