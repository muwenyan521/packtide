use anyhow::{Error, Result};

use super::{MirrorBackup, PrivilegedCommandRunner};

pub(super) struct RollbackGuard<'a, R: PrivilegedCommandRunner> {
    backup: &'a MirrorBackup,
    runner: &'a R,
    armed: bool,
}

impl<'a, R: PrivilegedCommandRunner> RollbackGuard<'a, R> {
    pub(super) fn new(backup: &'a MirrorBackup, runner: &'a R) -> Self {
        Self {
            backup,
            runner,
            armed: true,
        }
    }

    pub(super) fn rollback(&mut self) -> Result<()> {
        self.backup.restore(self.runner)?;
        self.armed = false;
        Ok(())
    }

    pub(super) fn commit(&mut self) {
        self.armed = false;
    }
}

impl<R: PrivilegedCommandRunner> Drop for RollbackGuard<'_, R> {
    fn drop(&mut self) {
        if self.armed {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.backup.restore_noninteractive(self.runner)
            })) {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("mirrorlist rollback failed: {error:#}"),
                Err(_) => eprintln!("mirrorlist rollback panicked"),
            }
        }
    }
}

pub(super) fn rollback_error<R: PrivilegedCommandRunner>(
    rollback: &mut RollbackGuard<'_, R>,
    error: Error,
    context: &str,
) -> Error {
    match rollback.rollback() {
        Ok(()) => error.context(context.to_owned()),
        Err(restore_error) => error.context(format!(
            "{context}; mirrorlist rollback failed: {restore_error:#}"
        )),
    }
}
