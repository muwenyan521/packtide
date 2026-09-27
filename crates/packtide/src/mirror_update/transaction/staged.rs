use std::ffi::OsString;
use std::path::PathBuf;

use super::PrivilegedCommandRunner;

pub(super) struct StagedMirror<'a, R: PrivilegedCommandRunner> {
    pub(super) path: PathBuf,
    runner: &'a R,
    armed: bool,
}

impl<'a, R: PrivilegedCommandRunner> StagedMirror<'a, R> {
    pub(super) fn new(path: PathBuf, runner: &'a R) -> Self {
        Self {
            path,
            runner,
            armed: true,
        }
    }

    pub(super) fn commit(&mut self) {
        self.armed = false;
    }
}

impl<R: PrivilegedCommandRunner> Drop for StagedMirror<'_, R> {
    fn drop(&mut self) {
        if self.armed {
            let args = [
                OsString::from("rm"),
                OsString::from("-f"),
                self.path.as_os_str().to_owned(),
            ];
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.runner.run_noninteractive(&args)
            })) {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("staged mirrorlist cleanup failed: {error:#}"),
                Err(_) => eprintln!("staged mirrorlist cleanup panicked"),
            }
        }
    }
}
