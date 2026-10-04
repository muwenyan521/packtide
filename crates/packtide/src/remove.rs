use anyhow::{Context, Result};
use std::env;
use std::time::{Instant, SystemTime};
use system_tools_core::{
    BackendId, BackendRegistry, PackageBackend, PackageIdentity, PackageKind, PackageScope,
    ReadOperation, TransactionAction, command_exists,
};

use crate::model::{PackageListing, PackageRecord};
use crate::transaction::{execute_flatpak, execute_native, execute_package_typed};
use crate::ui::parse_package_identity;
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
    let native = crate::app::native_backend("capability.remove")?;
    let pacman = if native == BackendId::Pacman {
        Some(pacman)
    } else {
        None
    };
    let helper = if native == BackendId::Pacman {
        Some(crate::app::package_helper_for("package removal")?)
    } else {
        None
    };
    let records = rows(pacman.as_deref(), helper, native)?;
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
    let Some(selected) = select_rows(
        helper.unwrap_or("native"),
        true,
        rows,
        query,
        Some(started_at),
    )?
    else {
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
    let mut package_ids = Vec::new();
    let mut flatpak_user = Vec::new();
    let mut flatpak_system = Vec::new();
    let mut native_groups: Vec<(BackendId, PackageKind, PackageScope, Vec<PackageIdentity>)> =
        Vec::new();
    for row in selected.lines() {
        let Some(_package) = parse_package_row(row) else {
            continue;
        };
        let Some(mut identity) = parse_package_identity(row) else {
            anyhow::bail!(
                "{}: selected row has invalid package identity",
                crate::locale::text(crate::locale::current(), "backend.unsupported", &[])
            );
        };
        if matches!(identity.backend, BackendId::Paru | BackendId::Yay)
            && let Some(helper) = helper
            && identity.backend
                != if helper == "yay" {
                    BackendId::Yay
                } else {
                    BackendId::Paru
                }
        {
            identity = identity_with_backend(
                &identity,
                if helper == "yay" {
                    BackendId::Yay
                } else {
                    BackendId::Paru
                },
            );
        }
        match identity.backend {
            BackendId::Flatpak => match identity.scope {
                PackageScope::System => flatpak_system.push(identity),
                PackageScope::User | PackageScope::Profile => flatpak_user.push(identity),
            },
            BackendId::Pacman | BackendId::Paru | BackendId::Yay => package_ids.push(identity),
            backend => push_native_group(&mut native_groups, backend, identity),
        }
    }
    if !package_ids.is_empty()
        && let Some(helper) = helper
    {
        execute_package_typed(helper, TransactionAction::Remove, &package_ids)?;
    }
    for (_, _, _, packages) in native_groups {
        execute_native(TransactionAction::Remove, &packages)?;
    }
    if command_exists("flatpak") {
        if std::env::var_os("PACKTIDE_DEBUG_REMOVE_ROWS").is_some() {
            eprintln!(
                "flatpak_records={:?}",
                records
                    .iter()
                    .map(|r| (&r.name, &r.repository, r.scope))
                    .collect::<Vec<_>>()
            );
        }
        if !flatpak_user.is_empty() {
            execute_flatpak(&flatpak_user)?;
        }
        if !flatpak_system.is_empty() {
            execute_flatpak(&flatpak_system)?;
        }
    }
    Ok(())
}

fn identity_with_backend(identity: &PackageIdentity, backend: BackendId) -> PackageIdentity {
    let mut result = PackageIdentity::new(
        backend,
        identity.kind,
        identity.scope,
        identity.native_key.clone(),
    );
    if let Some(origin) = &identity.origin {
        result = result.with_origin(origin.clone());
    }
    if let Some(display_name) = &identity.display_name {
        result = result.with_display_name(display_name.clone());
    }
    result
}

fn push_native_group(
    groups: &mut Vec<(BackendId, PackageKind, PackageScope, Vec<PackageIdentity>)>,
    backend: BackendId,
    identity: PackageIdentity,
) {
    if let Some((_, _, _, packages)) = groups.iter_mut().find(|(id, kind, scope, _)| {
        *id == backend && *kind == identity.kind && *scope == identity.scope
    }) {
        packages.push(identity);
    } else {
        groups.push((backend, identity.kind, identity.scope, vec![identity]));
    }
}

fn rows(
    pacman: Option<&std::path::Path>,
    helper: Option<&str>,
    native: BackendId,
) -> Result<Vec<crate::model::PackageRecord>> {
    if native != BackendId::Pacman {
        let registry = BackendRegistry::default();
        let provider = registry
            .backend(native)
            .ok_or_else(|| anyhow::anyhow!("native backend is not registered"))?;
        let installed = provider
            .read(ReadOperation::Installed)
            .map_err(|e| anyhow::anyhow!("native installed query failed: {e}"))?;
        return Ok(installed
            .packages
            .into_iter()
            .map(|package| {
                let name = package.native_key.as_str().to_owned();
                PackageRecord::from_identity(
                    package,
                    None,
                    name,
                    PackageListing::Version(String::new()),
                    true,
                )
            })
            .collect());
    }
    let pacman = pacman.expect("pacman path for Arch removal");
    let helper = helper.expect("AUR helper for Arch removal");
    let registry = BackendRegistry::default();
    let typed_installed = registry
        .backend(BackendId::Pacman)
        .ok_or_else(|| anyhow::anyhow!("backend pacman is not registered"))?
        .read(ReadOperation::Installed)
        .map_err(|error| anyhow::anyhow!("pacman typed installed read failed: {error}"))?;
    let foreign = if helper == "yay" {
        BackendId::Yay
    } else {
        BackendId::Paru
    };
    let foreign_installed = registry
        .backend(foreign)
        .ok_or_else(|| anyhow::anyhow!("backend {} is not registered", foreign.as_str()))?
        .read(ReadOperation::Installed)
        .with_context(|| format!("{} typed installed read failed", foreign.as_str()))?
        .packages;
    let foreign_names = foreign_installed
        .iter()
        .map(|package| package.native_key.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut records = typed_installed
        .packages
        .into_iter()
        .filter(|package| !foreign_names.contains(package.native_key.as_str()))
        .map(|package| {
            let name = package.native_key.as_str().to_owned();
            PackageRecord::from_identity(
                package,
                None,
                name,
                PackageListing::Version(String::new()),
                true,
            )
        })
        .collect::<Vec<_>>();
    records.extend(foreign_installed.into_iter().map(|package| {
        let name = package.native_key.as_str().to_owned();
        PackageRecord::from_identity(
            package,
            Some("aur".to_owned()),
            name,
            PackageListing::Version(String::new()),
            true,
        )
    }));
    if command_exists("flatpak") {
        let typed_flatpak = registry
            .backend(BackendId::Flatpak)
            .ok_or_else(|| anyhow::anyhow!("backend flatpak is not registered"))?
            .read(ReadOperation::Installed)
            .map_err(|error| anyhow::anyhow!("Flatpak typed installed read failed: {error}"))?
            .packages;
        records.extend(typed_flatpak.into_iter().map(|package| {
            let name = package.native_key.as_str().to_owned();
            let repository =
                (package.scope == PackageScope::System).then(|| "flatpak@system".to_owned());
            let listing = PackageListing::Flatpak {
                app_name: package.display_name.clone().unwrap_or_else(|| name.clone()),
                origin: package.origin.clone().unwrap_or_default(),
            };
            PackageRecord::from_identity(package, repository, name, listing, true)
        }));
    }
    let _ = pacman;
    Ok(records)
}
