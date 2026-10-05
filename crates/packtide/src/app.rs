use anyhow::Result;
use clap::{CommandFactory, Parser};
use std::env;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};
use system_tools_core::ExecutableResolver;
use system_tools_core::{BackendId, backend_from_os_release};

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
        let lang = crate::locale::current();
        let capability_key = match capability {
            "package details" | "downgrade package details" => "capability.details",
            "package installation" => "capability.install",
            "package removal" => "capability.remove",
            _ => "capability.catalog",
        };
        anyhow::bail!(
            "{} ({}): required AUR helper ('paru' or 'yay') is unavailable for {capability}; install paru or yay and retry",
            crate::locale::text(lang, "backend.missing_tool", &[]),
            crate::locale::text(lang, capability_key, &[]),
        )
    }
}

pub(crate) fn native_backend(capability: &str) -> Result<BackendId> {
    let os_release = fs::read_to_string("/etc/os-release").map_err(|error| {
        anyhow::anyhow!("cannot detect native package backend for {capability}: {error}")
    })?;
    let native = backend_from_os_release(&os_release).map_err(|error| {
        anyhow::anyhow!("cannot detect native package backend for {capability}: {error}")
    })?;
    let resolver = ExecutableResolver::from_path(env::var_os("PATH").as_deref());
    let backend = native.backend_id(|command| resolver.resolve(command).is_some());
    let command = native_entry_command(backend);
    if ExecutableResolver::from_path(env::var_os("PATH").as_deref())
        .resolve(OsStr::new(command))
        .is_none()
    {
        let description = match capability {
            "capability.remove" => "the installed package lookup",
            _ => capability,
        };
        anyhow::bail!(
            "required command '{command}' is unavailable for {description}; install it and retry"
        );
    }
    Ok(backend)
}

fn native_entry_command(backend: BackendId) -> &'static str {
    match backend {
        BackendId::Apt => "apt-get",
        BackendId::Dnf4 => "dnf",
        BackendId::Xbps => "xbps-query",
        _ => backend.as_str(),
    }
}

pub(crate) fn require_command_for(
    command: &str,
    capability_key: &str,
    capability: &str,
    optional: bool,
) -> Result<std::path::PathBuf> {
    let lang = crate::locale::current();
    system_tools_core::resolve_command_for(command, capability, optional).map_err(|error| {
        anyhow::anyhow!(
            "{} ({}): {error}",
            crate::locale::text(lang, "backend.missing_tool", &[]),
            crate::locale::text(lang, capability_key, &[]),
        )
    })
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

#[cfg(test)]
mod tests {
    use super::native_entry_command;
    use system_tools_core::BackendId;

    #[test]
    fn native_entry_uses_provider_executable_names() {
        assert_eq!(native_entry_command(BackendId::Xbps), "xbps-query");
        assert_eq!(native_entry_command(BackendId::Dnf4), "dnf");
        assert_eq!(native_entry_command(BackendId::Apt), "apt-get");
    }
}
