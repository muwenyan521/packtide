use anyhow::{Result, bail};
use std::collections::HashMap;
use std::env;
use std::time::SystemTime;
use system_tools_core::{
    BackendId, CommandPrivilege, ExecutableResolver, PackageBackend, PackageIdentity, PackageScope,
    TransactionAction,
};

use crate::source_metadata::{PackageRoute, package_route};
use crate::sources::install_catalog_for_backends;
use crate::transaction::{execute_native, execute_package, execute_package_typed};
use crate::ui::{parse_package_identity, parse_package_row, write_install_catalog};

pub(crate) fn run(query: &[String], refresh: bool) -> Result<()> {
    let started_at = SystemTime::now();
    crate::app::require_command_for(
        "fzf",
        "capability.install",
        "the package installation picker",
        false,
    )?;
    let native = crate::app::native_backend("capability.install")?;
    if native == BackendId::Pacman {
        return run_pacman(query, refresh, started_at);
    }
    {
        // All detected backends use the same typed catalog/query path. This keeps
        // Arch list-only and interactive pickers in lockstep, including optional
        // providers (AUR/Flatpak) and query filtering.
        let records = install_catalog_for_backends(native, refresh, query)?;
        let rows = crate::ui::render_package_rows(&records, crate::ui::PackageListMode::Install);
        if env::var_os("PACKTIDE_INSTALL_LIST_ONLY").is_some() {
            println!("{rows}");
            return Ok(());
        }
        let selected = crate::ui::select_rows("native", false, rows, query, Some(started_at))?;
        let Some(selected) = selected else {
            return Ok(());
        };
        let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
        let registry = system_tools_core::BackendRegistry::default();
        let mut grouped: HashMap<
            (BackendId, PackageScope, CommandPrivilege),
            Vec<PackageIdentity>,
        > = HashMap::new();
        for row in selected.lines() {
            if let Some(identity) = parse_package_identity(row) {
                let privilege = registry
                    .backend(identity.backend)
                    .ok_or_else(|| {
                        anyhow::anyhow!("backend {} is not registered", identity.backend.as_str())
                    })?
                    .write_with_resolver(
                        system_tools_core::WriteOperation::transaction(
                            TransactionAction::Install,
                            vec![identity.clone()],
                        ),
                        &resolver,
                    )?
                    .command
                    .privilege;
                grouped
                    .entry((identity.backend, identity.scope, privilege))
                    .or_default()
                    .push(identity);
            }
        }
        for ((backend, _, _), packages) in grouped {
            if backend == BackendId::Pacman {
                execute_package_typed("pacman", TransactionAction::Install, &packages)?;
            } else {
                execute_native(TransactionAction::Install, &packages)?;
            }
        }
        Ok(())
    }
}

fn run_pacman(query: &[String], refresh: bool, started_at: SystemTime) -> Result<()> {
    let pacman = crate::app::require_command_for(
        "pacman",
        "capability.catalog",
        "the package catalog lookup",
        false,
    )?;
    let helper = crate::app::package_helper_for("package installation")?;
    if env::var_os("PACKTIDE_INSTALL_LIST_ONLY").is_some() {
        let catalog =
            crate::sources::install_rows_streaming(&pacman, refresh, query, true, |_| Ok(()))?;
        let mut output = std::io::stdout().lock();
        write_install_catalog(&catalog, &mut output)?;
        let optional = crate::sources::optional_install_rows(query, refresh)?;
        if !optional.is_empty() {
            use std::io::Write;
            let rows =
                crate::ui::render_package_rows(&optional, crate::ui::PackageListMode::Install);
            if !catalog.records.is_empty() || !catalog.aur_names.trim().is_empty() {
                output.write_all(b"\n")?;
            }
            output.write_all(rows.as_bytes())?;
        }
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
    let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    let registry = system_tools_core::BackendRegistry::default();
    let mut optional: HashMap<(BackendId, PackageScope, CommandPrivilege), Vec<PackageIdentity>> =
        HashMap::new();
    for row in selected.lines() {
        let Some(package) = parse_package_row(row) else {
            continue;
        };
        let Some(identity) = parse_package_identity(row) else {
            bail!(
                "{}: install selection has invalid package identity",
                crate::locale::text(crate::locale::current(), "backend.unsupported", &[])
            );
        };
        match package_route(identity.backend) {
            PackageRoute::ArchForeign => {
                aur.push(identity.native_key.as_str().to_owned());
            }
            PackageRoute::ArchRepository => {
                let Some(repository) = package.repository else {
                    bail!(
                        "{}: install selection is missing its pacman repository",
                        crate::locale::text(crate::locale::current(), "backend.unsupported", &[])
                    );
                };
                let name = identity.native_key.as_str();
                repo.push(if name.contains('/') {
                    name.to_owned()
                } else {
                    format!("{repository}/{name}")
                });
            }
            PackageRoute::Flatpak | PackageRoute::Other => {
                let privilege = registry
                    .backend(identity.backend)
                    .ok_or_else(|| {
                        anyhow::anyhow!("backend {} is not registered", identity.backend.as_str())
                    })?
                    .write_with_resolver(
                        system_tools_core::WriteOperation::transaction(
                            TransactionAction::Install,
                            vec![identity.clone()],
                        ),
                        &resolver,
                    )?
                    .command
                    .privilege;
                optional
                    .entry((identity.backend, identity.scope, privilege))
                    .or_default()
                    .push(identity);
            }
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
    for ((_, _, _), identities) in optional {
        execute_native(TransactionAction::Install, &identities)?;
    }
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
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
