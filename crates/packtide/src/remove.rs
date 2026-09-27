use anyhow::Result;
use std::env;
use std::time::{Instant, SystemTime};
use system_tools_core::{
    PackageSource, TransactionAction, command_exists, require_command_for, resolve_command_for,
    run_capture_path,
};

use crate::sources::{flatpak_rows, parse_remove_rows};
use crate::transaction::{execute_flatpak, execute_package};
use crate::ui::{PackageListMode, parse_package_row, render_package_rows, select_rows};

pub(crate) fn run(query: &[String]) -> Result<()> {
    let started = Instant::now();
    let started_at = SystemTime::now();
    require_command_for("fzf", "the package removal picker", false)?;
    let pacman = resolve_command_for("pacman", "the installed package lookup", false)?;
    let helper = crate::app::package_helper_for("package removal")?;
    let records = rows(&pacman)?;
    let rows = render_package_rows(&records, PackageListMode::Remove);
    if env::var_os("SYSTEM_TOOLS_DEBUG_TIMINGS").is_some() {
        eprintln!(
            "picker_timing picker=remove phase=prepared elapsed_ms={}",
            started.elapsed().as_millis()
        );
    }
    if env::var_os("PACKTIDE_REMOVE_LIST_ONLY").is_some() {
        println!("{rows}");
        return Ok(());
    }
    let Some(selected) = select_rows(helper, true, rows, query, Some(started_at))? else {
        println!(
            "{}",
            crate::locale::text(crate::locale::current(), "selection.remove.none", &[])
        );
        return Ok(());
    };
    let mut pacman = Vec::new();
    let mut flatpak = Vec::new();
    for row in selected.lines() {
        let Some(package) = parse_package_row(row) else {
            continue;
        };
        match package.source {
            PackageSource::Flatpak => {
                flatpak.push(package.name);
            }
            PackageSource::Pacman | PackageSource::Aur => {
                pacman.push(package.name);
            }
        }
    }
    if !pacman.is_empty() {
        execute_package(helper, TransactionAction::Remove, &pacman)?;
    }
    if !flatpak.is_empty() && command_exists("flatpak") {
        execute_flatpak(&flatpak)?;
    }
    Ok(())
}

fn rows(pacman: &std::path::Path) -> Result<Vec<crate::model::PackageRecord>> {
    let pacman_for_installed = pacman.to_path_buf();
    let pacman_for_sync = pacman.to_path_buf();
    let (installed, repo_names, flatpak) = std::thread::scope(|scope| -> Result<_> {
        let installed = scope.spawn(|| {
            run_capture_path(&pacman_for_installed, &["--color=never", "-Q"], false)
                .map(|output| output.stdout)
        });
        let repo_names = scope.spawn(|| {
            Ok::<_, anyhow::Error>(
                run_capture_path(&pacman_for_sync, &["--color=never", "-Sl"], true)
                    .map(|output| output.stdout)
                    .unwrap_or_default(),
            )
        });
        let flatpak = scope.spawn(|| Ok::<_, anyhow::Error>(flatpak_rows()));
        Ok((
            installed
                .join()
                .map_err(|_| anyhow::anyhow!("installed package query thread panicked"))??,
            repo_names
                .join()
                .map_err(|_| anyhow::anyhow!("repository package query thread panicked"))??,
            flatpak
                .join()
                .map_err(|_| anyhow::anyhow!("Flatpak query thread panicked"))??,
        ))
    })?;
    Ok(parse_remove_rows(&installed, &repo_names, &flatpak))
}
