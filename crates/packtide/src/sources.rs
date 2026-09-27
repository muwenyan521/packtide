mod aur;
mod flatpak;
mod pacman;

use crate::model::PackageRecord;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

#[cfg(test)]
pub(crate) use pacman::parse_install_rows;

pub(crate) struct InstallCatalog {
    pub(crate) official: Vec<PackageRecord>,
    pub(crate) official_names: HashSet<String>,
    pub(crate) aur_names: String,
    pub(crate) installed: HashSet<String>,
}

pub(crate) fn install_rows(pacman: &Path, refresh: bool) -> Result<InstallCatalog> {
    install_rows_streaming(pacman, refresh, true, |_| Ok(()))
}

pub(crate) fn install_rows_streaming(
    pacman: &Path,
    refresh: bool,
    retain_official: bool,
    mut write_official: impl FnMut(&PackageRecord) -> Result<()>,
) -> Result<InstallCatalog> {
    let started = Instant::now();
    let installed_child = Command::new(pacman)
        .args(["-Qq"])
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to execute {} -Qq", pacman.display()))?;
    let mut catalog_child = match Command::new(pacman)
        .args(["--color=never", "-Sl"])
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let _ = installed_child.wait_with_output();
            return Err(error).context(format!("failed to execute {} -Sl", pacman.display()));
        }
    };
    let installed = match installed_child.wait_with_output() {
        Ok(output) => output,
        Err(error) => {
            let _ = catalog_child.wait();
            return Err(error).context("failed waiting for pacman installed package query");
        }
    };
    if !installed.status.success() {
        let _ = catalog_child.wait();
        anyhow::bail!(
            "pacman installed package query exited with {}",
            installed.status
        );
    }
    let installed = match String::from_utf8(installed.stdout) {
        Ok(output) => output,
        Err(error) => {
            let _ = catalog_child.wait();
            return Err(error).context("pacman installed package query returned invalid UTF-8");
        }
    };
    let installed = installed
        .lines()
        .filter(|name| valid_package_name(name))
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    let mut catalog = InstallCatalog {
        official: Vec::new(),
        official_names: HashSet::new(),
        aur_names: String::new(),
        installed,
    };
    let stdout = catalog_child
        .stdout
        .take()
        .context("cannot open pacman catalog output")?;
    for line in BufReader::new(stdout).lines() {
        let line = line?;
        let Some(record) = pacman::parse_install_line(&line, &catalog.installed) else {
            continue;
        };
        write_official(&record)?;
        catalog.official_names.insert(record.name.clone());
        if retain_official {
            catalog.official.push(record);
        }
    }
    let status = catalog_child.wait()?;
    if !status.success() {
        anyhow::bail!("pacman catalog exited with {status}");
    }
    catalog.aur_names = aur::fetch_names_for_picker(refresh, false).unwrap_or_default();
    if std::env::var_os("SYSTEM_TOOLS_DEBUG_TIMINGS").is_some() {
        eprintln!(
            "source_timing source=install_rows elapsed_ms={}",
            started.elapsed().as_millis()
        );
    }
    Ok(catalog)
}

pub(crate) fn parse_remove_rows(
    installed: &str,
    sync: &str,
    flatpak_rows: &str,
) -> Vec<PackageRecord> {
    let mut rows = pacman::parse_remove_rows(installed, sync);
    rows.extend(flatpak::parse_remove_rows(flatpak_rows));
    rows
}

pub(crate) fn flatpak_rows() -> String {
    let started = Instant::now();
    if !system_tools_core::command_exists("flatpak") {
        return String::new();
    }
    let rows = system_tools_core::run_capture(
        "flatpak",
        &["list", "--app", "--columns=application,origin,name"],
        true,
    )
    .map(|output| output.stdout)
    .unwrap_or_default();
    if std::env::var_os("SYSTEM_TOOLS_DEBUG_TIMINGS").is_some() {
        eprintln!(
            "source_timing source=flatpak_rows elapsed_ms={}",
            started.elapsed().as_millis()
        );
    }
    rows
}

pub(crate) fn valid_package_name(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@._+-".contains(&byte))
}
