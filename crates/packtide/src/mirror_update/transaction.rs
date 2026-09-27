use anyhow::{Context, Result, bail};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use system_tools_core::run_privileged;

mod rollback;
mod staged;
#[cfg(test)]
mod tests;

use rollback::{RollbackGuard, rollback_error};
use staged::StagedMirror;

pub(super) trait PrivilegedCommandRunner {
    fn run(&self, args: &[OsString]) -> Result<()>;

    fn run_noninteractive(&self, args: &[OsString]) -> Result<()> {
        self.run(args)
    }
}

pub(super) struct SystemPrivilegedCommandRunner;

impl PrivilegedCommandRunner for SystemPrivilegedCommandRunner {
    fn run(&self, args: &[OsString]) -> Result<()> {
        run_privileged(args).map(|_| ())
    }

    fn run_noninteractive(&self, args: &[OsString]) -> Result<()> {
        let args = std::iter::once(OsString::from("-n"))
            .chain(args.iter().cloned())
            .collect::<Vec<_>>();
        run_privileged(&args).map(|_| ())
    }
}

pub(super) struct MirrorBackup {
    directory: PathBuf,
    destination: PathBuf,
    backup: PathBuf,
    original_existed: bool,
}

impl MirrorBackup {
    pub(super) fn new(destination: &Path, directory: &Path) -> Result<Self> {
        fs::create_dir_all(directory).context("cannot create mirror backup directory")?;
        let backup = directory.join("mirrorlist.bak");
        let original_existed = destination.is_file();
        if original_existed {
            fs::copy(destination, &backup).context("cannot back up mirrorlist")?;
        }
        Ok(Self {
            directory: directory.to_path_buf(),
            destination: destination.to_path_buf(),
            backup,
            original_existed,
        })
    }

    pub(super) fn backup_path(&self) -> &Path {
        &self.backup
    }

    pub(super) fn install<R>(&self, content: &str, runner: &R) -> Result<()>
    where
        R: PrivilegedCommandRunner,
    {
        if content.trim().is_empty() {
            bail!("generated mirrorlist is empty");
        }
        let temporary = self.directory.join("mirrorlist.new");
        fs::write(&temporary, content).context("cannot write generated mirrorlist")?;
        let mut staged = self.stage_file(&temporary, runner)?;
        let metadata = fs::metadata(&staged.path).context("cannot verify staged mirrorlist")?;
        if metadata.len() == 0 {
            bail!("generated mirrorlist is empty");
        }
        let mut rollback = RollbackGuard::new(self, runner);
        if let Err(error) = self.publish(&staged.path, runner) {
            return Err(rollback_error(
                &mut rollback,
                error,
                "cannot publish generated mirrorlist",
            ));
        }
        let metadata = match fs::metadata(&self.destination) {
            Ok(metadata) => metadata,
            Err(error) => {
                return Err(rollback_error(
                    &mut rollback,
                    error.into(),
                    "cannot verify published mirrorlist",
                ));
            }
        };
        if metadata.len() == 0 {
            return Err(rollback_error(
                &mut rollback,
                anyhow::anyhow!("generated mirrorlist is empty"),
                "cannot publish generated mirrorlist",
            ));
        }
        rollback.commit();
        staged.commit();
        Ok(())
    }

    fn staging_path(&self) -> Result<PathBuf> {
        let parent = self
            .destination
            .parent()
            .context("mirrorlist destination has no parent")?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before the Unix epoch")?
            .as_nanos();
        Ok(parent.join(format!(
            ".mirrorlist.packtide-{}-{nonce}",
            std::process::id()
        )))
    }

    fn stage_file<'a, R>(&self, source: &Path, runner: &'a R) -> Result<StagedMirror<'a, R>>
    where
        R: PrivilegedCommandRunner,
    {
        let source_size = fs::metadata(source)
            .with_context(|| format!("cannot inspect staged source {}", source.display()))?
            .len();
        let staged = StagedMirror::new(self.staging_path()?, runner);
        let args = [
            OsString::from("install"),
            OsString::from("-m"),
            OsString::from("644"),
            source.as_os_str().to_owned(),
            staged.path.as_os_str().to_owned(),
        ];
        runner
            .run(&args)
            .context("cannot stage mirrorlist for atomic publication")?;
        let staged_size = fs::metadata(&staged.path)
            .with_context(|| format!("cannot inspect staged mirrorlist {}", staged.path.display()))?
            .len();
        if staged_size != source_size {
            bail!("staged mirrorlist size does not match its source");
        }
        Ok(staged)
    }

    fn publish<R>(&self, staged_path: &Path, runner: &R) -> Result<()>
    where
        R: PrivilegedCommandRunner,
    {
        let args = [
            OsString::from("mv"),
            OsString::from("-f"),
            staged_path.as_os_str().to_owned(),
            self.destination.as_os_str().to_owned(),
        ];
        runner.run(&args)
    }

    fn restore<R>(&self, runner: &R) -> Result<()>
    where
        R: PrivilegedCommandRunner,
    {
        if self.original_existed {
            if !self.backup.is_file() {
                bail!("mirrorlist backup is missing: {}", self.backup.display());
            }
            let mut staged = self.stage_file(&self.backup, runner)?;
            self.publish(&staged.path, runner)?;
            staged.commit();
            Ok(())
        } else {
            let args = [
                OsString::from("rm"),
                OsString::from("-f"),
                self.destination.as_os_str().to_owned(),
            ];
            runner.run(&args)
        }
    }

    fn restore_noninteractive<R>(&self, runner: &R) -> Result<()>
    where
        R: PrivilegedCommandRunner,
    {
        if self.original_existed {
            if !self.backup.is_file() {
                bail!("mirrorlist backup is missing: {}", self.backup.display());
            }
            let mut staged = self.stage_file(&self.backup, runner)?;
            let args = [
                OsString::from("mv"),
                OsString::from("-f"),
                staged.path.as_os_str().to_owned(),
                self.destination.as_os_str().to_owned(),
            ];
            runner.run_noninteractive(&args)?;
            staged.commit();
            Ok(())
        } else {
            let args = [
                OsString::from("rm"),
                OsString::from("-f"),
                self.destination.as_os_str().to_owned(),
            ];
            runner.run_noninteractive(&args)
        }
    }
}
