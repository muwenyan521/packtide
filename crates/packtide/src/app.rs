use anyhow::Result;
use clap::{CommandFactory, Parser};
use std::env;
use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};
use system_tools_core::ExecutableResolver;

use crate::cli::{Cli, CommandLine, CompatibilityRoute, compatibility_route};
use crate::commands::sysup;
use crate::ui::preview_command;

pub(crate) fn package_helper() -> Result<&'static str> {
    package_helper_for("package details")
}

pub(crate) fn package_helper_for(capability: &str) -> Result<&'static str> {
    let resolver = ExecutableResolver::from_path(env::var_os("PATH").as_deref());
    if resolver.resolve(OsStr::new("paru")).is_some() {
        Ok("paru")
    } else if resolver.resolve(OsStr::new("yay")).is_some() {
        Ok("yay")
    } else {
        anyhow::bail!(
            "required AUR helper ('paru' or 'yay') is unavailable for {capability}; install paru or yay and retry"
        )
    }
}

pub(crate) fn run() -> Result<()> {
    let raw_args = env::args().skip(1).collect::<Vec<_>>();
    if raw_args.first().is_some_and(|arg| arg == "__timing-mark") {
        let path = raw_args
            .get(1)
            .ok_or_else(|| anyhow::anyhow!("timing marker path is missing"))?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(mut file) => file.write_all(now.to_string().as_bytes())?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        return Ok(());
    }
    match compatibility_route(&raw_args) {
        Some(CompatibilityRoute::Install { query, refresh }) => {
            return crate::install::run(&query, refresh);
        }
        Some(CompatibilityRoute::Preview(args)) => return preview_command(&args),
        None => {}
    }
    match Cli::parse().command {
        Some(CommandLine::CheckUpdates { refresh }) => crate::check_updates::run(refresh),
        Some(CommandLine::Install {
            query,
            refresh,
            no_ai: _,
        }) => crate::install::run(&query, refresh),
        Some(CommandLine::Remove { query }) => crate::remove::run(&query),
        Some(CommandLine::MirrorUpdate { country }) => crate::mirror_update::run(country),
        Some(CommandLine::Downgrade { query }) => crate::downgrade::run(&query),
        Some(CommandLine::Sysup {
            list,
            ui_lang,
            news_source,
            count,
        }) => sysup(list, &ui_lang, &news_source, count),
        None => Cli::command().print_help().map_err(Into::into),
    }
}
