use anyhow::{Context, Result};
use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

pub fn command_exists(command: &str) -> bool {
    let resolver = ExecutableResolver::from_path(env::var_os("PATH").as_deref());
    resolver.resolve(OsStr::new(command)).is_some()
}

pub fn current_executable() -> Result<PathBuf> {
    std::env::current_exe().context("cannot locate current executable")
}

pub struct ExecutableResolver {
    search_path: Vec<PathBuf>,
    cache: RefCell<HashMap<OsString, Option<PathBuf>>>,
}

impl ExecutableResolver {
    pub fn from_path(path: Option<&OsStr>) -> Self {
        let cwd = env::current_dir().ok();
        let search_path = path
            .map(env::split_paths)
            .into_iter()
            .flatten()
            .filter_map(|directory| {
                if directory.as_os_str().is_empty() {
                    cwd.clone()
                } else if directory.is_absolute() {
                    Some(directory)
                } else {
                    cwd.as_ref().map(|cwd| cwd.join(directory))
                }
            })
            .collect();
        Self {
            search_path,
            cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn resolve(&self, name: &OsStr) -> Option<PathBuf> {
        if let Some(resolved) = self.cache.borrow().get(name) {
            return resolved.clone();
        }
        let resolved = self
            .search_path
            .iter()
            .map(|directory| directory.join(name))
            .find(|candidate| is_executable(candidate));
        self.cache
            .borrow_mut()
            .insert(name.to_owned(), resolved.clone());
        resolved
    }
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        path.metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutableResolver;
    use std::ffi::OsStr;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn resolves_executable_from_explicit_path_without_reading_process_environment() {
        let directory =
            std::env::temp_dir().join(format!("system-tools-core-resolver-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("create fixture directory");
        let executable = directory.join("fake-tool");
        fs::write(&executable, "fixture").expect("create fixture executable");
        let mut permissions = fs::metadata(&executable)
            .expect("stat fixture executable")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("make fixture executable");
        let resolver = ExecutableResolver::from_path(Some(directory.as_os_str()));
        assert_eq!(resolver.resolve(OsStr::new("fake-tool")), Some(executable));
        fs::remove_dir_all(directory).expect("remove fixture directory");
    }

    #[test]
    fn does_not_resolve_non_executable_file() {
        let directory = std::env::temp_dir().join(format!(
            "system-tools-core-resolver-non-executable-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("create fixture directory");
        let file = directory.join("fake-tool");
        fs::write(&file, "fixture").expect("create non-executable fixture");
        let mut permissions = fs::metadata(&file)
            .expect("stat non-executable fixture")
            .permissions();
        permissions.set_mode(0o644);
        fs::set_permissions(&file, permissions).expect("make fixture non-executable");
        let resolver = ExecutableResolver::from_path(Some(directory.as_os_str()));

        assert_eq!(resolver.resolve(OsStr::new("fake-tool")), None);
        fs::remove_dir_all(directory).expect("remove fixture directory");
    }

    #[test]
    fn does_not_resolve_missing_executable() {
        let resolver = ExecutableResolver::from_path(None);
        assert_eq!(resolver.resolve(OsStr::new("missing-tool")), None);
    }

    #[test]
    fn anchors_relative_path_entries_and_empty_entries() {
        let name = format!("system-tools-core-relative-{}", std::process::id());
        let relative = std::path::PathBuf::from(&name);
        let absolute = std::env::current_dir()
            .expect("read current directory")
            .join(&relative);
        let executable = absolute.join("fake-tool");
        let _ = fs::remove_dir_all(&absolute);
        fs::create_dir_all(&absolute).expect("create fixture directory");
        fs::write(&executable, "fixture").expect("create fixture executable");
        let mut permissions = fs::metadata(&executable)
            .expect("stat fixture executable")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("make fixture executable");

        let relative_path = std::env::join_paths([relative.as_os_str()]).expect("encode PATH");
        assert_eq!(
            ExecutableResolver::from_path(Some(&relative_path)).resolve(OsStr::new("fake-tool")),
            Some(executable.clone())
        );
        let empty_path = std::env::join_paths([OsStr::new(""), absolute.as_os_str()])
            .expect("encode empty PATH entry");
        let resolved = ExecutableResolver::from_path(Some(&empty_path))
            .resolve(OsStr::new("fake-tool"))
            .expect("resolve through absolute entry");
        assert!(resolved.is_absolute());
        fs::remove_dir_all(absolute).expect("remove fixture directory");
    }
}
