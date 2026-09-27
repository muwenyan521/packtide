use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use std::env;
use std::io::Read;
use std::time::{Duration, Instant};
use system_tools_core::CacheStore;

pub(crate) fn fetch_names_for_picker(refresh: bool, background_refresh: bool) -> Result<String> {
    let started = Instant::now();
    let result = fetch_names_for_picker_inner(refresh, background_refresh);
    if env::var_os("SYSTEM_TOOLS_DEBUG_TIMINGS").is_some() {
        eprintln!(
            "source_timing source=aur_package_names elapsed_ms={}",
            started.elapsed().as_millis()
        );
    }
    result
}

fn fetch_names_for_picker_inner(refresh: bool, background_refresh: bool) -> Result<String> {
    let cache_dir = env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".cache")))
        .context("HOME or XDG_CACHE_HOME is required for AUR cache")?
        .join("packtide/aur");
    let cache = CacheStore::new(cache_dir)?;
    if !refresh {
        if let Some(contents) = cache.read_fresh("packages", Duration::from_secs(3600))? {
            return Ok(contents);
        }
        if let Some(contents) = cache.read("packages")? {
            if !background_refresh {
                return fetch_names_from_cache(&cache, true).or(Ok(contents));
            }
            return Ok(contents);
        }
    }
    if background_refresh {
        return Ok(String::new());
    }
    fetch_names_from_cache(&cache, true)
}

fn fetch_names_from_cache(cache: &CacheStore, refresh: bool) -> Result<String> {
    if !refresh && let Some(contents) = cache.read_fresh("packages", Duration::from_secs(3600))? {
        return Ok(contents);
    }
    let lock = cache.acquire_refresh_lock("packages")?;
    if !lock.is_owner() {
        return cache
            .read("packages")?
            .context("AUR cache refresh is already in progress");
    }
    let fetched = fetch_and_store(cache);
    match fetched {
        Ok(list) => Ok(list),
        Err(error) => cache
            .read("packages")?
            .filter(|contents| !contents.is_empty())
            .ok_or(error),
    }
}

fn fetch_and_store(cache: &CacheStore) -> Result<String> {
    let agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_body(Some(Duration::from_secs(30)))
        .timeout_global(Some(Duration::from_secs(45)))
        .build()
        .new_agent();
    let response = agent
        .get("https://aur.archlinux.org/packages.gz")
        .call()
        .context("AUR package list unavailable")?;
    const DOWNLOAD_LIMIT: u64 = 32 * 1024 * 1024;
    const EXPANDED_LIMIT: u64 = 128 * 1024 * 1024;
    let content_length = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if content_length.is_some_and(|size| size > DOWNLOAD_LIMIT) {
        bail!("AUR package list exceeds the 32 MiB download limit");
    }
    let compressed = response.into_body().into_reader().take(DOWNLOAD_LIMIT + 1);
    let mut decoder = GzDecoder::new(compressed);
    let mut expanded = decoder.by_ref().take(EXPANDED_LIMIT + 1);
    let mut list = String::new();
    expanded
        .read_to_string(&mut list)
        .context("invalid AUR package list")?;
    if list.len() as u64 > EXPANDED_LIMIT {
        bail!("AUR package list exceeds the 128 MiB decompressed limit");
    }
    cache.write_atomic("packages", &list)?;
    Ok(list)
}
