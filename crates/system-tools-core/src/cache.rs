use anyhow::{Context, Result};
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

pub struct CacheStore {
    root: PathBuf,
}

impl CacheStore {
    pub fn new(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root)
            .with_context(|| format!("cannot create cache directory {}", root.display()))?;
        Ok(Self { root })
    }

    pub fn read_fresh(&self, name: &str, ttl: Duration) -> Result<Option<String>> {
        let path = self.root.join(name);
        let mut file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Ok(None),
        };
        let fresh = file
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age < ttl);
        if !fresh {
            return Ok(None);
        }
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .with_context(|| format!("cannot read cache {}", path.display()))?;
        Ok(Some(contents))
    }

    pub fn read(&self, name: &str) -> Result<Option<String>> {
        let path = self.root.join(name);
        match std::fs::read_to_string(&path) {
            Ok(contents) => Ok(Some(contents)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => {
                Err(error).with_context(|| format!("cannot read cache {}", path.display()))
            }
        }
    }

    pub fn write_atomic(&self, name: &str, contents: &str) -> Result<()> {
        let path = self.root.join(name);
        let nonce = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let temporary = self
            .root
            .join(format!(".{name}.tmp-{}-{nonce}", std::process::id()));
        std::fs::write(&temporary, contents)
            .with_context(|| format!("cannot write cache {}", temporary.display()))?;
        std::fs::rename(&temporary, &path)
            .with_context(|| format!("cannot publish cache {}", path.display()))
    }

    pub fn acquire_refresh_lock(&self, name: &str) -> Result<RefreshLock> {
        let path = self.root.join(format!(".{name}.refresh.lock"));
        loop {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(RefreshLock { path: Some(path) }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let stale = std::fs::metadata(&path)
                        .and_then(|metadata| metadata.modified())
                        .ok()
                        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                        .is_some_and(|age| age > Duration::from_secs(300));
                    if !stale {
                        return Ok(RefreshLock { path: None });
                    }
                    let nonce = SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|duration| duration.as_nanos())
                        .unwrap_or_default();
                    let reclaimed = self.root.join(format!(
                        ".{name}.refresh.stale-{}-{nonce}",
                        std::process::id()
                    ));
                    match std::fs::rename(&path, &reclaimed) {
                        Ok(()) => {
                            let _ = std::fs::remove_file(reclaimed);
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(_) => return Ok(RefreshLock { path: None }),
                    }
                }
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("cannot create cache lock {}", path.display()));
                }
            }
        }
    }
}

pub struct RefreshLock {
    path: Option<PathBuf>,
}

impl RefreshLock {
    pub fn is_owner(&self) -> bool {
        self.path.is_some()
    }
}

impl Drop for RefreshLock {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CacheStore;
    use std::fs;

    #[test]
    fn cache_store_publishes_atomically_and_applies_ttl() {
        let directory =
            std::env::temp_dir().join(format!("system-tools-core-cache-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        let cache = CacheStore::new(directory.clone()).expect("create cache");
        cache.write_atomic("value", "payload").expect("write cache");
        assert_eq!(
            cache.read("value").expect("read cache"),
            Some("payload".to_owned())
        );
        assert_eq!(
            cache
                .read_fresh("value", std::time::Duration::from_secs(60))
                .expect("fresh cache"),
            Some("payload".to_owned())
        );
        fs::remove_dir_all(directory).expect("remove cache fixture");
    }

    #[test]
    fn concurrent_cache_writers_leave_a_complete_value() {
        let directory = std::env::temp_dir().join(format!(
            "system-tools-core-cache-concurrent-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        let first_root = directory.clone();
        let second_root = directory.clone();
        let first = std::thread::spawn(move || {
            CacheStore::new(first_root)
                .expect("create first cache")
                .write_atomic("value", "first")
                .expect("write first cache");
        });
        let second = std::thread::spawn(move || {
            CacheStore::new(second_root)
                .expect("create second cache")
                .write_atomic("value", "second")
                .expect("write second cache");
        });
        first.join().expect("first writer");
        second.join().expect("second writer");
        let value = fs::read_to_string(directory.join("value")).expect("read concurrent cache");
        assert!(value == "first" || value == "second");
        fs::remove_dir_all(directory).expect("remove cache fixture");
    }

    #[test]
    fn refresh_lock_is_exclusive_and_released_on_drop() {
        let directory = std::env::temp_dir().join(format!(
            "system-tools-core-cache-lock-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        let cache = CacheStore::new(directory.clone()).expect("create cache");
        let first = cache
            .acquire_refresh_lock("packages")
            .expect("acquire first lock");
        assert!(first.is_owner());
        let second = cache
            .acquire_refresh_lock("packages")
            .expect("observe held lock");
        assert!(!second.is_owner());
        drop(first);
        let third = cache
            .acquire_refresh_lock("packages")
            .expect("acquire after release");
        assert!(third.is_owner());
        drop(third);
        fs::remove_dir_all(directory).expect("remove cache fixture");
    }
}
