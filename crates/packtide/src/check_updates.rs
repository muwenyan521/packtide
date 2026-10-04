use anyhow::{Context, Result};
use std::collections::HashSet;
use std::env;
use std::io::{self, IsTerminal, Write};
use std::process::{Command, Stdio};
use std::time::Duration;
use std::time::Instant;
use system_tools_core::{
    BackendId, BackendRegistry, CacheStore, ExecutableResolver, PackageBackend, PackageSource,
    ReadOperation, current_executable,
};

use crate::commands::sysup;
use crate::model::{PackageUpdate, parse_cached_updates, render_update_rows};
use crate::transaction::refresh_waybar_cache;
use crate::ui::{
    COMMON_FZF_LAYOUT_ARGS, NO_SELECTION, classify_picker_status, picker_columns, shell_quote,
    wrap_shortcut_line,
};

pub(crate) fn run(refresh: bool) -> Result<()> {
    let picker_started = Instant::now();
    let list_only = env::var_os("PACKTIDE_UPDATE_LIST_ONLY").is_some();
    if refresh && !list_only {
        refresh_waybar_cache();
    }
    let cache = UpdateCache::new()?;
    let fresh_repo_aur = cache.read_repo_aur_fresh();
    let fresh = fresh_repo_aur.is_some();
    let interactive_terminal = io::stdout().is_terminal() && io::stdin().is_terminal();
    let force_interactive = test_force_interactive();
    let fzf = if !list_only && (interactive_terminal || force_interactive) {
        ExecutableResolver::from_path(env::var_os("PATH").as_deref())
            .resolve(std::ffi::OsStr::new("fzf"))
    } else {
        None
    };
    let interactive = fzf.is_some();
    let background_refresh = interactive && !refresh && !fresh;
    let repo_aur = if !refresh && fresh {
        fresh_repo_aur
            .or_else(query_arch_updates)
            .unwrap_or_default()
    } else if background_refresh {
        cache.read_repo_aur().unwrap_or_default()
    } else {
        cache.refresh_repo_aur()?
    };
    let updates = repo_aur
        .into_iter()
        .chain(query_native_updates())
        .chain(query_flatpak_updates())
        .collect::<Vec<_>>();
    let updates = deduplicate_updates(updates);
    if env::var_os("SYSTEM_TOOLS_DEBUG_TIMINGS").is_some() {
        eprintln!(
            "picker_timing picker=check-updates phase=prepared elapsed_ms={}",
            picker_started.elapsed().as_millis()
        );
    }
    if list_only {
        println!("{}", render_update_rows(&updates));
        return Ok(());
    }
    if updates.is_empty() {
        println!(
            "{}",
            crate::locale::text(crate::locale::current(), "updates.empty", &[])
        );
        return Ok(());
    }
    if interactive {
        let rows = render_update_rows(&updates);
        let executable = current_executable()?;
        let refresh_command = format!(
            "PACKTIDE_UPDATE_LIST_ONLY=1 {} check-updates --refresh",
            shell_quote(executable.to_string_lossy().as_ref())
        );
        let lang = crate::locale::current();
        let bind = format!(
            "ctrl-r:change-prompt({})+reload-sync({refresh_command})",
            crate::locale::text(lang, "updates.refresh", &[]),
        );
        let header = wrap_shortcut_line(
            crate::locale::text(lang, "updates.header", &[]).as_str(),
            picker_columns(),
        );
        let prompt = crate::locale::text(lang, "updates.prompt", &[]);
        let reset_prompt = format!("load:change-prompt({prompt})");
        let start_reload = background_refresh.then(|| format!("start:reload({refresh_command})"));
        let mut args = COMMON_FZF_LAYOUT_ARGS.to_vec();
        args.extend([
            "--delimiter=\t",
            "--nth=2",
            "--id-nth=2",
            "--track",
            "--info=inline",
            "--header",
            header.as_str(),
            "--prompt",
            prompt.as_str(),
            "--bind",
            bind.as_str(),
            "--bind",
            reset_prompt.as_str(),
        ]);
        if let Some(start_reload) = &start_reload {
            args.extend(["--bind", start_reload.as_str()]);
        }
        let mut child = Command::new(fzf.expect("interactive mode resolved fzf"))
            .args(args)
            .env("FZF_DEFAULT_OPTS", "")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .context("cannot start fzf update list")?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(rows.as_bytes())?;
        }
        if classify_picker_status(child.wait()?, "check-updates")? {
            return sysup(false, "auto", "official", 15);
        }
        println!("{NO_SELECTION}");
        return Ok(());
    }
    for item in &updates {
        let source = crate::locale::source_label(crate::locale::current(), item.source);
        println!(
            "[{:<7}] {} {}",
            source,
            item.name,
            item.version.as_deref().unwrap_or("")
        );
    }
    Ok(())
}

fn test_force_interactive() -> bool {
    #[cfg(debug_assertions)]
    {
        env::var_os("PACKTIDE_TEST_FORCE_INTERACTIVE").is_some()
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

fn deduplicate_updates(updates: Vec<PackageUpdate>) -> Vec<PackageUpdate> {
    let mut seen = HashSet::new();
    updates
        .into_iter()
        .filter(|item| seen.insert((item.source, item.name.clone())))
        .collect()
}

fn debug_source_timing(source: &str, started: Instant) {
    if env::var_os("SYSTEM_TOOLS_DEBUG_TIMINGS").is_some() {
        eprintln!(
            "source_timing source={source} elapsed_ms={}",
            started.elapsed().as_millis()
        );
    }
}

struct UpdateCache {
    cache: CacheStore,
}

impl UpdateCache {
    fn new() -> Result<Self> {
        let base = env::var_os("XDG_CACHE_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".cache"))
            })
            .context("HOME or XDG_CACHE_HOME is required for update cache")?;
        Ok(Self {
            cache: CacheStore::new(base.join("packtide/check-updates"))?,
        })
    }

    fn read_repo_aur_fresh(&self) -> Option<Vec<PackageUpdate>> {
        self.cache
            .read_fresh("updates.txt", Duration::from_secs(3600))
            .ok()
            .flatten()
            .map(|contents| parse_cached_updates(&contents))
    }

    fn read_repo_aur(&self) -> Option<Vec<PackageUpdate>> {
        Some(parse_cached_updates(&self.cache.read("updates.txt").ok()??))
    }

    fn write_repo_aur(&self, updates: &[PackageUpdate]) -> Result<()> {
        let mut contents = String::with_capacity(
            updates
                .iter()
                .map(|item| item.display.len() + item.source.as_str().len() + 1)
                .sum(),
        );
        for (index, item) in updates.iter().enumerate() {
            if index > 0 {
                contents.push('\n');
            }
            contents.push_str(item.source.as_str());
            contents.push('\t');
            contents.push_str(&item.display);
        }
        self.cache.write_atomic("updates.txt", &contents)
    }

    fn refresh_repo_aur(&self) -> Result<Vec<PackageUpdate>> {
        let lock = self.cache.acquire_refresh_lock("updates")?;
        if !lock.is_owner() {
            return Ok(self.read_repo_aur().unwrap_or_default());
        }
        match query_arch_updates() {
            Some(queried) => {
                self.write_repo_aur(&queried)?;
                Ok(queried)
            }
            None => Ok(self.read_repo_aur().unwrap_or_default()),
        }
    }
}

fn query_arch_updates() -> Option<Vec<PackageUpdate>> {
    let mut updates = Vec::new();
    let registry = BackendRegistry::default();
    let resolver = ExecutableResolver::from_path(env::var_os("PATH").as_deref());
    if resolver
        .resolve(std::ffi::OsStr::new("checkupdates"))
        .is_some()
    {
        let started = Instant::now();
        let typed = registry
            .backend(BackendId::Pacman)?
            .read(ReadOperation::Updates)
            .ok()?;
        updates.extend(typed.packages.into_iter().map(|package| PackageUpdate {
            source: PackageSource::Pacman,
            name: package.native_key.as_str().to_owned(),
            version: None,
            display: package.native_key.as_str().to_owned(),
        }));
        debug_source_timing("repo_updates", started);
    }
    let helper = resolver
        .resolve(std::ffi::OsStr::new("paru"))
        .or_else(|| resolver.resolve(std::ffi::OsStr::new("yay")));
    if let Some(helper) = helper {
        let started = Instant::now();
        let backend = if helper.file_name().is_some_and(|name| name == "yay") {
            BackendId::Yay
        } else {
            BackendId::Paru
        };
        let typed = registry
            .backend(backend)?
            .read(ReadOperation::Updates)
            .ok()?;
        updates.extend(typed.packages.into_iter().map(|package| {
            PackageUpdate {
                source: PackageSource::Aur,
                name: package
                    .native_key
                    .as_str()
                    .trim_start_matches("aur/")
                    .to_owned(),
                version: None,
                display: package.native_key.as_str().to_owned(),
            }
        }));
        debug_source_timing("aur_updates", started);
    }
    Some(updates)
}

fn query_native_updates() -> Vec<PackageUpdate> {
    let backend = crate::app::native_backend("capability.updates").ok();
    let Some(backend) = backend else {
        return Vec::new();
    };
    let registry = BackendRegistry::default();
    let Some(provider) = registry.backend(backend) else {
        return Vec::new();
    };
    provider
        .read(ReadOperation::Updates)
        .ok()
        .map(|result| {
            result
                .packages
                .into_iter()
                .map(|package| PackageUpdate {
                    source: match backend {
                        BackendId::Apt => PackageSource::Apt,
                        BackendId::Dnf5 | BackendId::Dnf4 => PackageSource::Dnf,
                        BackendId::Zypper => PackageSource::Zypper,
                        BackendId::Apk => PackageSource::Apk,
                        BackendId::Xbps => PackageSource::Xbps,
                        _ => PackageSource::Pacman,
                    },
                    name: package.native_key.as_str().to_owned(),
                    version: None,
                    display: package.native_key.as_str().to_owned(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn query_flatpak_updates() -> Vec<PackageUpdate> {
    let started = Instant::now();
    let registry = BackendRegistry::default();
    let resolver = ExecutableResolver::from_path(env::var_os("PATH").as_deref());
    let updates = resolver
        .resolve(std::ffi::OsStr::new("flatpak"))
        .and_then(|flatpak| {
            let typed = registry
                .backend(BackendId::Flatpak)?
                .read(ReadOperation::Updates)
                .ok()?;
            let _ = flatpak;
            Some(typed)
        })
        .map(|typed| {
            typed
                .packages
                .into_iter()
                .map(|package| PackageUpdate {
                    source: PackageSource::Flatpak,
                    name: package.native_key.as_str().to_owned(),
                    version: None,
                    display: package.native_key.as_str().to_owned(),
                })
                .collect()
        })
        .unwrap_or_default();
    debug_source_timing("flatpak_updates", started);
    updates
}

#[cfg(test)]
mod native_update_tests {
    use super::*;

    #[test]
    fn native_backend_sources_map_to_stable_picker_sources() {
        for (backend, source) in [
            (BackendId::Apt, PackageSource::Apt),
            (BackendId::Dnf5, PackageSource::Dnf),
            (BackendId::Zypper, PackageSource::Zypper),
            (BackendId::Apk, PackageSource::Apk),
            (BackendId::Xbps, PackageSource::Xbps),
        ] {
            let mapped = match backend {
                BackendId::Apt => PackageSource::Apt,
                BackendId::Dnf5 | BackendId::Dnf4 => PackageSource::Dnf,
                BackendId::Zypper => PackageSource::Zypper,
                BackendId::Apk => PackageSource::Apk,
                BackendId::Xbps => PackageSource::Xbps,
                _ => PackageSource::Pacman,
            };
            assert_eq!(mapped, source);
        }
    }
}
