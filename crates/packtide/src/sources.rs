mod aur;
mod flatpak;
mod pacman;

use crate::model::PackageRecord;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use system_tools_core::{
    BackendId, BackendRegistry, CacheStore, PackageBackend, ReadOperation, command_exists,
};

#[cfg(test)]
pub(crate) use pacman::parse_install_rows;

#[allow(dead_code)]
pub(crate) struct InstallCatalog {
    pub(crate) official: Vec<PackageRecord>,
    pub(crate) official_names: HashSet<String>,
    pub(crate) aur_names: String,
    pub(crate) installed: HashSet<String>,
    pub(crate) records: Vec<PackageRecord>,
}

const QUERY_MIN_CHARS: usize = 2;
const QUERY_CACHE_TTL: Duration = Duration::from_secs(30);
type QueryCache = HashMap<(BackendId, String), (Instant, Vec<PackageRecord>)>;

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedRecord {
    backend: String,
    kind: String,
    scope: String,
    key: String,
    origin: Option<String>,
    display: Option<String>,
    repository: Option<String>,
    name: String,
    installed: bool,
}

fn cache_store() -> Option<CacheStore> {
    let root = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".cache"))
        })?
        .join("packtide/query");
    CacheStore::new(root).ok()
}

fn cache_name(native: BackendId, query: &str) -> String {
    let mut name = format!("{}-", native.as_str());
    for byte in query.as_bytes() {
        name.push_str(&format!("{byte:02x}"));
    }
    name.push_str(".json");
    name
}

fn cache_decode(contents: &str) -> Option<Vec<PackageRecord>> {
    let values: Vec<CachedRecord> = serde_json::from_str(contents).ok()?;
    values
        .into_iter()
        .map(|value| {
            let backend = BackendId::ALL
                .into_iter()
                .find(|id| id.as_str() == value.backend)?;
            let kind = parse_kind(&value.kind)?;
            let scope = parse_scope(&value.scope)?;
            let key = system_tools_core::NativePackageKey::new(value.key).ok()?;
            let mut identity = system_tools_core::PackageIdentity::new(backend, kind, scope, key);
            if let Some(origin) = value.origin {
                identity = identity.with_origin(origin);
            }
            if let Some(display) = value.display {
                identity = identity.with_display_name(display);
            }
            Some(PackageRecord::from_identity(
                identity,
                value.repository,
                value.name,
                crate::model::PackageListing::Version("-".to_owned()),
                value.installed,
            ))
        })
        .collect()
}

fn kind_name(kind: system_tools_core::PackageKind) -> &'static str {
    match kind {
        system_tools_core::PackageKind::System => "system",
        system_tools_core::PackageKind::Aur => "aur",
        system_tools_core::PackageKind::Flatpak => "flatpak",
        system_tools_core::PackageKind::Snap => "snap",
        system_tools_core::PackageKind::BrewFormula => "brew-formula",
        system_tools_core::PackageKind::BrewCask => "brew-cask",
        system_tools_core::PackageKind::Nix => "nix",
    }
}
fn scope_name(scope: system_tools_core::PackageScope) -> &'static str {
    match scope {
        system_tools_core::PackageScope::System => "system",
        system_tools_core::PackageScope::User => "user",
        system_tools_core::PackageScope::Profile => "profile",
    }
}
fn parse_kind(value: &str) -> Option<system_tools_core::PackageKind> {
    system_tools_core::PackageKind::ALL
        .into_iter()
        .find(|k| kind_name(*k) == value)
}
fn parse_scope(value: &str) -> Option<system_tools_core::PackageScope> {
    system_tools_core::PackageScope::ALL
        .into_iter()
        .find(|s| scope_name(*s) == value)
}

fn cache_encode(records: &[PackageRecord]) -> Option<String> {
    let values = records
        .iter()
        .map(|record| CachedRecord {
            backend: record.backend.as_str().to_owned(),
            kind: kind_name(record.kind).to_owned(),
            scope: scope_name(record.scope).to_owned(),
            key: record.native_key.as_str().to_owned(),
            origin: record.origin.clone(),
            display: record.display_name.clone(),
            repository: record.repository.clone(),
            name: record.name.clone(),
            installed: record.installed,
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&values).ok()
}

fn write_query_cache(backend: BackendId, query: &str, records: &[PackageRecord]) {
    let Some(store) = cache_store() else { return };
    let Some(contents) = cache_encode(records) else {
        return;
    };
    if let Err(error) = store.write_atomic(&cache_name(backend, query), &contents) {
        eprintln!(
            "query cache write failed for {}: {error:#}",
            backend.as_str()
        );
    }
}

fn query_cache() -> &'static Mutex<QueryCache> {
    static CACHE: OnceLock<Mutex<QueryCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn query_is_eligible(query: &str) -> bool {
    query.chars().count() >= QUERY_MIN_CHARS
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
    let normalized_query = query.join(" ");
    if !refresh
        && query_is_eligible(&normalized_query)
        && let Ok(cache) = query_cache().lock()
        && let Some((at, records)) = cache.get(&(native, normalized_query.clone()))
        && at.elapsed() <= QUERY_CACHE_TTL
    {
        return Ok(records.clone());
    }
    if !refresh
        && query_is_eligible(&normalized_query)
        && let Some(store) = cache_store()
        && let Ok(Some(contents)) =
            store.read_fresh(&cache_name(native, &normalized_query), QUERY_CACHE_TTL)
        && let Some(records) = cache_decode(&contents)
    {
        if let Ok(mut cache) = query_cache().lock() {
            cache.insert(
                (native, normalized_query.clone()),
                (Instant::now(), records.clone()),
            );
        }
        return Ok(records);
    }
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
            && ((!query.is_empty() && query_is_eligible(&normalized_query))
                || !matches!(id, BackendId::Snap | BackendId::Nix))
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
    if !refresh
        && query_is_eligible(&normalized_query)
        && let Ok(mut cache) = query_cache().lock()
    {
        cache.insert(
            (native, normalized_query.clone()),
            (Instant::now(), records.clone()),
        );
        cache.retain(|_, (at, _)| at.elapsed() <= QUERY_CACHE_TTL);
    }
    if !refresh && query_is_eligible(&normalized_query) {
        write_query_cache(native, &normalized_query, &records);
    }
    Ok(records)
}

pub(crate) fn optional_install_rows(query: &[String], refresh: bool) -> Result<Vec<PackageRecord>> {
    let normalized = query
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if !refresh
        && query_is_eligible(&normalized)
        && let Some(store) = cache_store()
        && let Ok(Some(contents)) = store.read_fresh(
            &cache_name(BackendId::Flatpak, &format!("optional:{normalized}")),
            QUERY_CACHE_TTL,
        )
        && let Some(records) = cache_decode(&contents)
    {
        return Ok(records);
    }
    let operation = if normalized.is_empty() {
        ReadOperation::Catalog
    } else {
        ReadOperation::Search {
            query: normalized.clone(),
        }
    };
    let registry = BackendRegistry::default();
    let mut ids = Vec::new();
    if command_exists("flatpak") {
        ids.push(BackendId::Flatpak);
    }
    for (id, command) in [
        (BackendId::Snap, "snap"),
        (BackendId::Brew, "brew"),
        (BackendId::Nix, "nix"),
    ] {
        if command_exists(command)
            && (query_is_eligible(&normalized) || !matches!(id, BackendId::Snap | BackendId::Nix))
        {
            ids.push(id);
        }
    }
    let mut records = Vec::new();
    let mut diagnostics = Vec::new();
    for backend in ids {
        let Some(provider) = registry.backend(backend) else {
            continue;
        };
        let result = match provider.read(operation.clone()) {
            Ok(result) => result,
            Err(error) => {
                diagnostics.push(format!("{}: {error}", backend.as_str()));
                continue;
            }
        };
        for identity in result.packages {
            let name = identity
                .display_name
                .clone()
                .unwrap_or_else(|| identity.native_key.as_str().to_owned());
            records.push(PackageRecord::from_identity(
                identity,
                None,
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
    if !refresh && query_is_eligible(&normalized) {
        if let Ok(mut cache) = query_cache().lock() {
            cache.insert(
                (BackendId::Flatpak, format!("optional:{normalized}")),
                (Instant::now(), records.clone()),
            );
        }
        write_query_cache(
            BackendId::Flatpak,
            &format!("optional:{normalized}"),
            &records,
        );
    }
    Ok(records)
}

#[allow(dead_code)]
pub(crate) fn install_rows(pacman: &Path, refresh: bool) -> Result<InstallCatalog> {
    install_rows_streaming(pacman, refresh, &[], true, |_| Ok(()))
}

#[allow(dead_code)]
pub(crate) fn install_rows_streaming(
    _pacman: &Path,
    refresh: bool,
    query: &[String],
    retain_official: bool,
    mut write_official: impl FnMut(&PackageRecord) -> Result<()>,
) -> Result<InstallCatalog> {
    let started = Instant::now();
    let registry = BackendRegistry::default();
    let normalized_query = query
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let operation = if query_is_eligible(&normalized_query) {
        ReadOperation::Search {
            query: normalized_query.clone(),
        }
    } else if refresh {
        ReadOperation::RefreshCatalog
    } else {
        ReadOperation::Catalog
    };
    let stream_cache_query = format!("arch-stream:{normalized_query}");
    let cached_records = if !refresh && query_is_eligible(&normalized_query) {
        cache_store()
            .and_then(|store| {
                store
                    .read_fresh(
                        &cache_name(BackendId::Pacman, &stream_cache_query),
                        QUERY_CACHE_TTL,
                    )
                    .ok()
                    .flatten()
            })
            .and_then(|contents| cache_decode(&contents))
    } else {
        None
    };
    let cache_hit = cached_records.is_some();
    let (typed_packages, installed) = if let Some(records) = cached_records {
        let installed = records
            .iter()
            .filter(|record| record.installed)
            .map(|record| record.name.clone())
            .collect();
        (records, installed)
    } else {
        let typed_catalog = registry
            .backend(BackendId::Pacman)
            .ok_or_else(|| anyhow::anyhow!("backend pacman is not registered"))?
            .read(operation)
            .map_err(|error| {
                anyhow::anyhow!("pacman catalog exited: typed read failed: {error}")
            })?;
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
        (
            typed_catalog
                .packages
                .into_iter()
                .map(|package| {
                    let (repository, name) = package
                        .native_key
                        .as_str()
                        .split_once('/')
                        .map(|(repo, name)| (repo.to_owned(), name.to_owned()))
                        .unwrap_or_else(|| (String::new(), package.native_key.as_str().to_owned()));
                    let installed_state = installed.contains(&name);
                    if repository.is_empty() {
                        PackageRecord::legacy(
                            system_tools_core::PackageSource::Pacman,
                            None,
                            name,
                            crate::model::PackageListing::Version("-".to_owned()),
                            installed_state,
                        )
                    } else {
                        PackageRecord::from_identity(
                            package,
                            Some(repository),
                            name,
                            crate::model::PackageListing::Version("-".to_owned()),
                            installed_state,
                        )
                    }
                })
                .collect(),
            installed,
        )
    };
    let mut catalog = InstallCatalog {
        official: Vec::new(),
        official_names: HashSet::new(),
        aur_names: String::new(),
        installed,
        records: Vec::new(),
    };
    for record in typed_packages {
        if record.source != system_tools_core::PackageSource::Pacman {
            continue;
        }
        write_official(&record)?;
        catalog.official_names.insert(record.name.clone());
        if retain_official {
            catalog.official.push(record.clone());
        }
        catalog.records.push(record);
    }
    if !cache_hit && !refresh && query_is_eligible(&normalized_query) {
        write_query_cache(BackendId::Pacman, &stream_cache_query, &catalog.records);
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
    use super::{
        install_catalog_for_backends, install_rows_streaming, query_is_eligible, write_query_cache,
    };
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use system_tools_core::BackendId;

    #[test]
    fn query_cache_roundtrips_across_child_process_without_provider_spawn() {
        if std::env::var_os("PACKTIDE_QUERY_CACHE_WRITE_CHILD").is_some() {
            let record = crate::model::PackageRecord::legacy(
                system_tools_core::PackageSource::Apt,
                None,
                "cached-package".to_owned(),
                crate::model::PackageListing::Version("-".to_owned()),
                false,
            );
            write_query_cache(BackendId::Apt, "cache-hit", &[record]);
            return;
        }
        if std::env::var_os("PACKTIDE_QUERY_CACHE_READ_CHILD").is_some() {
            let records =
                install_catalog_for_backends(BackendId::Apt, false, &["cache-hit".to_owned()])
                    .expect("cache hit");
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].name, "cached-package");
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "packtide-query-cache-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let exe = std::env::current_exe().unwrap();
        let write = std::process::Command::new(&exe).args(["--exact", "sources::tests::query_cache_roundtrips_across_child_process_without_provider_spawn", "--nocapture"]).env("PACKTIDE_QUERY_CACHE_WRITE_CHILD", "1").env("XDG_CACHE_HOME", &root).output().unwrap();
        assert!(
            write.status.success(),
            "{}",
            String::from_utf8_lossy(&write.stderr)
        );
        let output = std::process::Command::new(exe).args(["--exact", "sources::tests::query_cache_roundtrips_across_child_process_without_provider_spawn", "--nocapture"]).env("PACKTIDE_QUERY_CACHE_READ_CHILD", "1").env("XDG_CACHE_HOME", &root).env("PATH", "/nonexistent").output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn query_policy_requires_two_unicode_characters() {
        assert!(!query_is_eligible(""));
        assert!(!query_is_eligible("h"));
        assert!(query_is_eligible("he"));
        assert!(!query_is_eligible("好"));
        assert!(query_is_eligible("中文"));
    }

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

    #[test]
    fn streaming_query_uses_typed_pacman_search() {
        if std::env::var_os("PACKTIDE_STREAMING_QUERY_CHILD").is_some() {
            let pacman =
                std::env::var_os("PACKTIDE_STREAMING_QUERY_PACMAN").expect("pacman fixture path");
            let catalog = install_rows_streaming(
                Path::new(&pacman),
                false,
                &["  bash  ".to_owned()],
                false,
                |_| Ok(()),
            )
            .expect("streaming query catalog");
            assert_eq!(
                catalog
                    .records
                    .iter()
                    .map(|record| record.name.as_str())
                    .collect::<Vec<_>>(),
                ["bash"]
            );
            return;
        }

        let directory = std::env::temp_dir().join(format!(
            "packtide-streaming-query-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&directory).expect("create fixture directory");
        let pacman = directory.join("pacman");
        fs::write(
            &pacman,
            "#!/bin/sh\ncase \"$1 $2\" in\n  *-Sl) printf '%s\\n' 'core bash 5.2' 'extra vim 9.1' ;;\n  -Qq*) printf '%s\\n' bash ;;\nesac\n",
        )
        .expect("write fake pacman");
        fs::set_permissions(&pacman, fs::Permissions::from_mode(0o755))
            .expect("make fake pacman executable");
        let path = std::env::join_paths([
            directory.as_path(),
            Path::new("/usr/bin"),
            Path::new("/bin"),
        ])
        .expect("build fixture PATH");
        let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "sources::tests::streaming_query_uses_typed_pacman_search",
                "--nocapture",
            ])
            .env("PACKTIDE_STREAMING_QUERY_CHILD", "1")
            .env("PACKTIDE_STREAMING_QUERY_PACMAN", &pacman)
            .env("PATH", path)
            .output()
            .expect("run source child");
        assert!(
            output.status.success(),
            "child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let _ = fs::remove_dir_all(directory);
    }
}
