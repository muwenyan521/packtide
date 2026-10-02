use anyhow::Result;
use std::env;
use std::time::{Instant, SystemTime};
use system_tools_core::{
    BackendId, BuiltinBackend, PackageBackend, PackageSource, ReadOperation, TransactionAction,
    command_exists,
};

use crate::model::{PackageListing, PackageRecord};
use crate::transaction::{execute_flatpak, execute_package};
use crate::ui::{PackageListMode, parse_package_row, render_package_rows, select_rows};

pub(crate) fn run(query: &[String]) -> Result<()> {
    let started = Instant::now();
    let started_at = SystemTime::now();
    crate::app::require_command_for(
        "fzf",
        "capability.remove",
        "the package removal picker",
        false,
    )?;
    let pacman = crate::app::require_command_for(
        "pacman",
        "capability.catalog",
        "the installed package lookup",
        false,
    )?;
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
    if selected.is_empty() {
        println!(
            "{}",
            crate::locale::text(crate::locale::current(), "selection.remove.empty", &[])
        );
        return Ok(());
    }
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
    let typed_installed = BuiltinBackend::new(BackendId::Pacman)
        .read(ReadOperation::Installed)
        .map_err(|error| anyhow::anyhow!("pacman typed installed read failed: {error}"))?;
    let mut records = typed_installed
        .packages
        .into_iter()
        .map(|package| PackageRecord {
            source: PackageSource::Pacman,
            repository: None,
            name: package.native_key.as_str().to_owned(),
            listing: PackageListing::Version(String::new()),
            installed: true,
        })
        .collect::<Vec<_>>();
    if command_exists("flatpak") {
        let typed_flatpak = BuiltinBackend::new(BackendId::Flatpak)
            .read(ReadOperation::Installed)
            .map_err(|error| anyhow::anyhow!("Flatpak typed installed read failed: {error}"))?
            .packages;
        records.extend(typed_flatpak.into_iter().map(|package| {
            PackageRecord {
                source: PackageSource::Flatpak,
                repository: None,
                name: package.native_key.as_str().to_owned(),
                listing: PackageListing::Flatpak {
                    app_name: package
                        .display_name
                        .unwrap_or_else(|| package.native_key.as_str().to_owned()),
                    origin: package.origin.unwrap_or_default(),
                },
                installed: true,
            }
        }));
    }
    let _ = pacman;
    Ok(records)
}
