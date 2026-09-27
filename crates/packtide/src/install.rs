use anyhow::{Result, bail};
use std::env;
use std::time::{Instant, SystemTime};
use system_tools_core::{
    PackageSource, TransactionAction, require_command_for, resolve_command_for,
};

use crate::sources::install_rows;
use crate::transaction::execute_package;
use crate::ui::{NO_SELECTION, parse_package_row, write_install_catalog};

pub(crate) fn run(query: &[String], refresh: bool) -> Result<()> {
    let started = Instant::now();
    let started_at = SystemTime::now();
    require_command_for("fzf", "the package installation picker", false)?;
    let pacman = resolve_command_for("pacman", "the package catalog lookup", false)?;
    let helper = crate::app::package_helper_for("package installation")?;
    if env::var_os("SYSTEM_TOOLS_DEBUG_TIMINGS").is_some() {
        eprintln!(
            "picker_timing picker=install phase=prepared elapsed_ms={}",
            started.elapsed().as_millis()
        );
    }
    if env::var_os("PACKTIDE_INSTALL_LIST_ONLY").is_some() {
        let catalog = install_rows(&pacman, refresh)?;
        write_install_catalog(&catalog, std::io::stdout().lock())?;
        println!();
        return Ok(());
    }
    let Some(selected) = crate::ui::select_install_catalog_streaming(
        helper,
        &pacman,
        refresh,
        query,
        Some(started_at),
    )?
    else {
        println!("{NO_SELECTION}");
        return Ok(());
    };
    let mut repo = Vec::new();
    let mut aur = Vec::new();
    for row in selected.lines() {
        let Some(package) = parse_package_row(row) else {
            continue;
        };
        match package.source {
            PackageSource::Aur => aur.push(package.name),
            PackageSource::Pacman => {
                let Some(repository) = package.repository else {
                    bail!("install selection is missing its pacman repository");
                };
                repo.push(format!("{repository}/{}", package.name));
            }
            PackageSource::Flatpak => bail!("Flatpak rows are not valid in the package installer"),
        }
    }
    if !repo.is_empty() {
        execute_package(helper, TransactionAction::Install, &repo)?;
    }
    if !aur.is_empty() {
        execute_package(
            helper,
            TransactionAction::Install,
            &aur.iter()
                .map(|name| format!("aur/{name}"))
                .collect::<Vec<_>>(),
        )?;
    }
    Ok(())
}
