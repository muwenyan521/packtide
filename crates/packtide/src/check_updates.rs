use anyhow::{Context, Result};
use std::collections::HashSet;
use std::env;
use std::io::{self, IsTerminal, Write};
use std::process::{Command, Stdio};
use std::time::Duration;
use std::time::Instant;
use system_tools_core::{
    CacheStore, ExecutableResolver, PackageSource, current_executable, run_capture_path,
};

use crate::commands::sysup;
use crate::model::{
    PackageUpdate, parse_cached_updates, parse_flatpak, parse_updates, render_update_rows,
};
use crate::transaction::refresh_waybar_cache;
use crate::ui::{COMMON_FZF_LAYOUT_ARGS, NO_SELECTION, classify_picker_status, shell_quote};

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
    let force_interactive = env::var_os("PACKTIDE_FORCE_INTERACTIVE").is_some();
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
            "ctrl-r:change-prompt({})+reload-sync({refresh_command})+change-prompt({})",
            crate::locale::text(lang, "updates.refresh", &[]),
            crate::locale::text(lang, "updates.prompt", &[])
        );
        let header = crate::locale::text(lang, "updates.header", &[]);
        let prompt = crate::locale::text(lang, "updates.prompt", &[]);
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
        let source = match item.source {
            PackageSource::Pacman => "Pacman",
            PackageSource::Aur => "AUR",
            PackageSource::Flatpak => "Flatpak",
        };
        println!(
            "[{:<7}] {} {}",
            source,
            item.name,
            item.version.as_deref().unwrap_or("")
        );
    }
    Ok(())
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
    let resolver = ExecutableResolver::from_path(env::var_os("PATH").as_deref());
    if let Some(checkupdates) = resolver.resolve(std::ffi::OsStr::new("checkupdates")) {
        let started = Instant::now();
        let output = run_capture_path(&checkupdates, &[] as &[&str], true).ok()?;
        if output.status.success() {
            updates.extend(parse_updates(PackageSource::Pacman, &output.stdout));
        } else if output.status.code() != Some(2) {
            return None;
        }
        debug_source_timing("repo_updates", started);
    }
    let helper = resolver
        .resolve(std::ffi::OsStr::new("paru"))
        .or_else(|| resolver.resolve(std::ffi::OsStr::new("yay")));
    if let Some(helper) = helper {
        let started = Instant::now();
        let output = run_capture_path(&helper, &["-Qua"], true).ok()?;
        if !output.status.success() {
            return None;
        }
        updates.extend(parse_updates(PackageSource::Aur, &output.stdout));
        debug_source_timing("aur_updates", started);
    }
    Some(updates)
}

fn query_flatpak_updates() -> Vec<PackageUpdate> {
    let started = Instant::now();
    let resolver = ExecutableResolver::from_path(env::var_os("PATH").as_deref());
    let updates = resolver
        .resolve(std::ffi::OsStr::new("flatpak"))
        .and_then(|flatpak| {
            run_capture_path(
                &flatpak,
                &["remote-ls", "--updates", "--columns=application,version"],
                true,
            )
            .ok()
        })
        .map(|output| parse_flatpak(&output.stdout))
        .unwrap_or_default();
    debug_source_timing("flatpak_updates", started);
    updates
}
