mod aur;
mod flatpak;
mod pacman;

use crate::model::PackageRecord;
use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use std::time::Instant;
use system_tools_core::{
    BackendId, BackendRegistry, PackageBackend, ReadOperation, command_exists,
};

#[cfg(test)]
pub(crate) use pacman::parse_install_rows;

pub(crate) struct InstallCatalog {
    pub(crate) official: Vec<PackageRecord>,
    pub(crate) official_names: HashSet<String>,
    pub(crate) aur_names: String,
    pub(crate) installed: HashSet<String>,
    pub(crate) records: Vec<PackageRecord>,
}

pub(crate) fn install_catalog_for_backends(
    native: BackendId,
    refresh: bool,
    query: &[String],
) -> Result<Vec<PackageRecord>> {
    let registry = BackendRegistry::default();
    let query = query
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let operation = if query.is_empty() {
        if refresh {
            ReadOperation::RefreshCatalog
        } else {
            ReadOperation::Catalog
        }
    } else {
        ReadOperation::Search {
            query: query.join(" "),
        }
    };
    let mut ids = vec![native];
    if native == BackendId::Pacman {
        if command_exists("paru") {
            ids.push(BackendId::Paru);
        } else if command_exists("yay") {
            ids.push(BackendId::Yay);
        }
    }
    if command_exists("flatpak") {
        ids.push(BackendId::Flatpak);
    }
    for (id, command) in [
        (BackendId::Snap, "snap"),
        (BackendId::Brew, "brew"),
        (BackendId::Nix, "nix"),
    ] {
        if command_exists(command)
            && (!query.is_empty() || !matches!(id, BackendId::Snap | BackendId::Nix))
        {
            ids.push(id);
        }
    }
    let registry = &registry;
    let results = std::thread::scope(|scope| {
        ids.into_iter()
            .enumerate()
            .map(|(index, backend)| {
                let operation = operation.clone();
                scope.spawn(move || {
                    let result = registry
                        .backend(backend)
                        .map(|provider| provider.read(operation));
                    (index, backend, result)
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().expect("provider read thread panicked"))
            .collect::<Vec<_>>()
    });
    let mut results = results;
    results.sort_by_key(|(index, _, _)| *index);
    let mut records = Vec::new();
    let mut diagnostics = Vec::new();
    for (_, backend, result) in results {
        let Some(result) = result else {
            continue;
        };
        let result = match result {
            Ok(value) => value,
            Err(error) if backend != native => {
                diagnostics.push(format!("{}: {error}", backend.as_str()));
                continue;
            }
            Err(error) => {
                return Err(anyhow::anyhow!(
                    "{} catalog failed: {error}",
                    backend.as_str()
                ));
            }
        };
        for identity in result.packages {
            let name = identity
                .display_name
                .clone()
                .unwrap_or_else(|| identity.native_key.as_str().to_owned());
            let repository = match backend {
                BackendId::Pacman => identity
                    .native_key
                    .as_str()
                    .split_once('/')
                    .map(|(repo, _)| repo.to_owned()),
                BackendId::Paru | BackendId::Yay => Some("aur".to_owned()),
                _ => None,
            };
            let name = if backend == BackendId::Pacman {
                identity
                    .native_key
                    .as_str()
                    .split_once('/')
                    .map_or(name, |(_, n)| n.to_owned())
            } else {
                name
            };
            records.push(PackageRecord::from_identity(
                identity,
                repository,
                name,
                crate::model::PackageListing::Version("-".to_owned()),
                false,
            ));
        }
    }
    if !diagnostics.is_empty() {
        eprintln!(
            "optional package providers unavailable: {}",
            diagnostics.join("; ")
        );
    }
    Ok(records)
}

pub(crate) fn install_rows(pacman: &Path, refresh: bool) -> Result<InstallCatalog> {
    install_rows_streaming(pacman, refresh, true, |_| Ok(()))
}

pub(crate) fn install_rows_streaming(
    _pacman: &Path,
    refresh: bool,
    retain_official: bool,
    mut write_official: impl FnMut(&PackageRecord) -> Result<()>,
) -> Result<InstallCatalog> {
    let started = Instant::now();
    let registry = BackendRegistry::default();
    let typed_catalog = registry
        .backend(BackendId::Pacman)
        .ok_or_else(|| anyhow::anyhow!("backend pacman is not registered"))?
        .read(if refresh {
            ReadOperation::RefreshCatalog
        } else {
            ReadOperation::Catalog
        })
        .map_err(|error| anyhow::anyhow!("pacman catalog exited: typed read failed: {error}"))?;
    let installed = Command::new(_pacman)
        .args(["-Qq"])
        .output()
        .map_err(|error| anyhow::anyhow!("failed to execute pacman -Qq: {error}"))?;
    if !installed.status.success() {
        anyhow::bail!(
            "pacman installed package query exited with {}",
            installed.status
        );
    }
    let installed = String::from_utf8(installed.stdout)
        .map_err(|error| {
            anyhow::anyhow!("pacman installed package query returned invalid UTF-8: {error}")
        })?
        .lines()
        .filter(|name| valid_package_name(name))
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    let mut catalog = InstallCatalog {
        official: Vec::new(),
        official_names: HashSet::new(),
        aur_names: String::new(),
        installed,
        records: Vec::new(),
    };
    for package in typed_catalog.packages {
        let Some((repository, name)) = package.native_key.as_str().split_once('/') else {
            continue;
        };
        let record = PackageRecord::legacy(
            system_tools_core::PackageSource::Pacman,
            Some(repository.to_owned()),
            name.to_owned(),
            crate::model::PackageListing::Version("-".to_owned()),
            catalog.installed.contains(name),
        );
        write_official(&record)?;
        catalog.official_names.insert(record.name.clone());
        if retain_official {
            catalog.official.push(record.clone());
        }
        catalog.records.push(record);
    }
    catalog.aur_names = aur::fetch_names_for_picker(refresh, false)?;
    if !catalog.aur_names.trim().is_empty() {
        let aur_backend = if system_tools_core::command_exists("paru") {
            BackendId::Paru
        } else {
            BackendId::Yay
        };
        let typed_aur = registry
            .backend(aur_backend)
            .ok_or_else(|| anyhow::anyhow!("backend {} is not registered", aur_backend.as_str()))?
            .read(ReadOperation::Catalog)
            .map_err(|error| anyhow::anyhow!("AUR catalog exited: typed read failed: {error}"))?;
        let typed_aur_names = typed_aur
            .packages
            .iter()
            .map(|package| package.native_key.as_str())
            .collect::<HashSet<_>>();
        catalog.aur_names = catalog
            .aur_names
            .lines()
            .filter(|name| typed_aur_names.contains(*name))
            .collect::<Vec<_>>()
            .join("\n");
    }
    if std::env::var_os("SYSTEM_TOOLS_DEBUG_TIMINGS").is_some() {
        eprintln!(
            "source_timing source=install_rows elapsed_ms={}",
            started.elapsed().as_millis()
        );
    }
    Ok(catalog)
}

#[allow(dead_code)]
pub(crate) fn parse_remove_rows(
    installed: &str,
    sync: &str,
    flatpak_rows: &str,
) -> Vec<PackageRecord> {
    let mut rows = pacman::parse_remove_rows(installed, sync);
    rows.extend(flatpak::parse_remove_rows(flatpak_rows));
    rows
}

#[allow(dead_code)]
pub(crate) fn flatpak_rows() -> String {
    let started = Instant::now();
    if !system_tools_core::command_exists("flatpak") {
        return String::new();
    }
    let rows = system_tools_core::run_capture(
        "flatpak",
        &[
            "list",
            "--app",
            "--columns=application,origin,name,installation",
        ],
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

#[cfg(test)]
mod tests {
    use super::install_catalog_for_backends;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use system_tools_core::BackendId;

    #[test]
    fn blank_query_is_not_sent_to_query_required_optional_providers() {
        if std::env::var_os("PACKTIDE_SOURCE_QUERY_CHILD").is_some() {
            let native = std::env::var("PACKTIDE_SOURCE_QUERY_NATIVE").expect("native backend");
            let native = native.parse::<u8>().expect("backend marker");
            let backend = match native {
                1 => BackendId::Apt,
                _ => panic!("unsupported child backend"),
            };
            install_catalog_for_backends(backend, false, &["   ".to_owned()])
                .expect("blank query source lookup");
            return;
        }
        let directory = std::env::temp_dir().join(format!(
            "packtide-source-query-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&directory).expect("create fixture directory");
        let log = directory.join("provider.log");
        for name in ["apt-get", "apt-cache", "dpkg-query", "snap", "nix"] {
            let path = directory.join(name);
            let body = if matches!(name, "snap" | "nix") {
                format!(
                    "#!/bin/sh\nprintf '%s\\n' {name} >> '{}'\nexit 99\n",
                    log.display()
                )
            } else {
                "#!/bin/sh\ncase \"$1\" in dumpavail|--version|-Qq) exit 0;; *) exit 0;; esac\n"
                    .to_owned()
            };
            fs::write(&path, body).expect("write fake provider");
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))
                .expect("make fake provider executable");
        }
        let path = std::env::join_paths([
            directory.as_path(),
            Path::new("/usr/bin"),
            Path::new("/bin"),
        ])
        .expect("build fixture PATH");
        let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "sources::tests::blank_query_is_not_sent_to_query_required_optional_providers",
                "--nocapture",
            ])
            .env("PACKTIDE_SOURCE_QUERY_CHILD", "1")
            .env("PACKTIDE_SOURCE_QUERY_NATIVE", "1")
            .env("PATH", path)
            .output()
            .expect("run source child");
        assert!(
            output.status.success(),
            "child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !log.exists(),
            "query-required providers were invoked for blank query"
        );
        let _ = fs::remove_dir_all(directory);
    }
}
