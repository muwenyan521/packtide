use anyhow::{Context, Result, bail};
use std::ffi::OsString;
use std::process::Command;
use system_tools_core::{ExecutableResolver, current_executable};

pub(crate) fn sysup(list: bool, ui_lang: &str, news_source: &str, count: usize) -> Result<()> {
    let systide = systide_executable()?;
    let mut args = vec![OsString::from("--ui-lang"), OsString::from(ui_lang)];
    args.extend([OsString::from("--news-source"), OsString::from(news_source)]);
    args.extend([OsString::from("--count"), OsString::from(count.to_string())]);
    if list {
        args.push(OsString::from("--list"));
    }
    let status = Command::new(&systide)
        .args(&args)
        .status()
        .with_context(|| format!("failed to execute {} for system update", systide.display()))?;
    if status.success() {
        Ok(())
    } else {
        bail!("systide system update exited with {status}");
    }
}

fn systide_executable() -> Result<std::path::PathBuf> {
    if let Some(path) = test_systide_override()
        && path.is_file()
    {
        return Ok(path);
    }
    let current = current_executable()?;
    if let Some(sibling) = current.parent().map(|parent| parent.join("systide"))
        && sibling.is_file()
    {
        return Ok(sibling);
    }
    ExecutableResolver::from_path(std::env::var_os("PATH").as_deref())
        .resolve(std::ffi::OsStr::new("systide"))
        .ok_or_else(|| {
            anyhow::anyhow!("required command 'systide' is unavailable for system update")
        })
}

fn test_systide_override() -> Option<std::path::PathBuf> {
    #[cfg(debug_assertions)]
    {
        std::env::var_os("PACKTIDE_TEST_SYSTIDE_BIN").map(std::path::PathBuf::from)
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}
