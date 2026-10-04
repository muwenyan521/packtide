use anyhow::{Result, bail};
use std::env;
use std::time::{Instant, SystemTime};
use system_tools_core::{PackageSource, TransactionAction};

use crate::sources::install_rows;
use crate::transaction::execute_package;
use crate::ui::{parse_package_row, write_install_catalog};

pub(crate) fn run(query: &[String], refresh: bool) -> Result<()> {
    let started = Instant::now();
    let started_at = SystemTime::now();
    crate::app::require_command_for(
        "fzf",
        "capability.install",
        "the package installation picker",
        false,
    )?;
    let pacman = crate::app::require_command_for(
        "pacman",
        "capability.catalog",
        "the package catalog lookup",
        false,
    )?;
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
    let selected = crate::ui::select_install_catalog_streaming(
        helper,
        &pacman,
        refresh,
        query,
        Some(started_at),
    );
    let selected = match selected {
        Ok(selected) => selected,
        Err(error) => {
            let key = picker_failure_key(&error);
            let message = crate::locale::text(
                crate::locale::current(),
                key,
                &[("error", &error.to_string())],
            );
            eprintln!("\x1b[1;31m{message}\x1b[0m");
            return Err(error);
        }
    };
    let Some(selected) = selected else {
        println!(
            "{}",
            crate::locale::text(crate::locale::current(), "selection.install.none", &[])
        );
        return Ok(());
    };
    if selected.is_empty() {
        println!(
            "{}",
            crate::locale::text(crate::locale::current(), "selection.install.empty", &[])
        );
        return Ok(());
    }
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
                    bail!(
                        "{}: install selection is missing its pacman repository",
                        crate::locale::text(crate::locale::current(), "backend.unsupported", &[])
                    );
                };
                repo.push(format!("{repository}/{}", package.name));
            }
            PackageSource::Flatpak => bail!(
                "{}: Flatpak rows are not valid in the package installer",
                crate::locale::text(crate::locale::current(), "backend.unsupported", &[])
            ),
            _ => bail!(
                "{}: this package source is not available in the current picker",
                crate::locale::text(crate::locale::current(), "backend.unsupported", &[])
            ),
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

fn picker_failure_key(error: &anyhow::Error) -> &'static str {
    if error
        .chain()
        .any(|cause| cause.to_string().starts_with("failed writing"))
    {
        "selection.install.writer_failed"
    } else {
        "selection.install.source_failed"
    }
}

#[cfg(test)]
mod tests {
    use super::picker_failure_key;

    #[test]
    fn distinguishes_install_source_and_picker_writer_failures() {
        let source = anyhow::anyhow!("pacman catalog exited");
        let writer = anyhow::anyhow!("broken pipe").context("failed writing package row to fzf");

        assert_eq!(
            picker_failure_key(&source),
            "selection.install.source_failed"
        );
        assert_eq!(
            picker_failure_key(&writer),
            "selection.install.writer_failed"
        );
    }
}
