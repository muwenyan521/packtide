use super::{MirrorPaths, MirrorUpdate, ReflectorProvider};
use crate::mirror_update::transaction::PrivilegedCommandRunner;
use anyhow::{Context, Result, bail};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

struct FakeReflector {
    countries: String,
    responses: RefCell<VecDeque<Option<String>>>,
    requests: RefCell<Vec<Option<String>>>,
}

impl ReflectorProvider for FakeReflector {
    fn countries(&self) -> Result<String> {
        Ok(self.countries.clone())
    }

    fn mirrors(&self, country: Option<&str>) -> Result<Option<String>> {
        self.requests.borrow_mut().push(country.map(str::to_owned));
        self.responses
            .borrow_mut()
            .pop_front()
            .context("missing fake reflector response")
    }
}

struct FakePrivilegedRunner {
    calls: RefCell<Vec<Vec<OsString>>>,
}

impl PrivilegedCommandRunner for FakePrivilegedRunner {
    fn run(&self, args: &[OsString]) -> Result<()> {
        self.calls.borrow_mut().push(args.to_vec());
        match args.first().map(|arg| arg.to_string_lossy()) {
            Some(command) if command == "install" => {
                fs::copy(Path::new(&args[3]), Path::new(&args[4]))?;
                Ok(())
            }
            Some(command) if command == "mv" => {
                fs::rename(Path::new(&args[2]), Path::new(&args[3]))?;
                Ok(())
            }
            Some(command) if command == "rm" => {
                let target = Path::new(&args[2]);
                if target.exists() {
                    fs::remove_file(target)?;
                }
                Ok(())
            }
            _ => bail!("unexpected fake privileged command"),
        }
    }
}

fn fixture(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "packtide-mirror-flow-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create fixture");
    path
}

fn update<'a>(
    country: &str,
    directory: &Path,
    reflector: &'a FakeReflector,
    privileged: &'a FakePrivilegedRunner,
) -> MirrorUpdate<'a, FakeReflector, FakePrivilegedRunner> {
    MirrorUpdate {
        country: Some(country.to_owned()),
        paths: MirrorPaths {
            mirrorlist: directory.join("mirrorlist"),
            backup_dir: directory.join("cache"),
        },
        reflector,
        privileged,
    }
}

#[test]
fn update_uses_regional_fallback_before_global_mirrors() {
    // Given a recognized country with no country-specific results but regional results
    let directory = fixture("regional-fallback");
    let target = directory.join("mirrorlist");
    fs::write(&target, "original").expect("write original mirrorlist");
    let reflector = FakeReflector {
        countries: "China\nJapan\n".to_owned(),
        responses: RefCell::new(VecDeque::from([
            None,
            Some("Server = https://regional.example/$repo/os/$arch\n".to_owned()),
        ])),
        requests: RefCell::new(Vec::new()),
    };
    let privileged = FakePrivilegedRunner {
        calls: RefCell::new(Vec::new()),
    };

    // When the mirror update flow runs for China
    update("China", &directory, &reflector, &privileged)
        .execute()
        .expect("regional mirror update");

    // Then it tries China, then its region, and atomically publishes the result
    assert_eq!(
        *reflector.requests.borrow(),
        [
            Some("China".to_owned()),
            Some("China,Hong Kong,Taiwan,Japan,Singapore".to_owned())
        ]
    );
    assert_eq!(
        fs::read_to_string(target).expect("read updated mirrorlist"),
        "Server = https://regional.example/$repo/os/$arch\n"
    );
    assert_eq!(privileged.calls.borrow().len(), 2);
    fs::remove_dir_all(directory).expect("remove fixture");
}

#[test]
fn update_keeps_original_when_country_and_global_mirrors_fail() {
    // Given recognized country, regional fallback, and global requests all fail
    let directory = fixture("all-fail");
    let target = directory.join("mirrorlist");
    fs::write(&target, "original").expect("write original mirrorlist");
    let reflector = FakeReflector {
        countries: "China\n".to_owned(),
        responses: RefCell::new(VecDeque::from([None, None, None])),
        requests: RefCell::new(Vec::new()),
    };
    let privileged = FakePrivilegedRunner {
        calls: RefCell::new(Vec::new()),
    };

    // When the mirror update flow exhausts all sources
    let result = update("China", &directory, &reflector, &privileged).execute();

    // Then it fails without changing the target or invoking privileged writes
    assert!(result.is_err());
    assert_eq!(
        fs::read_to_string(&target).expect("read original mirrorlist"),
        "original"
    );
    assert_eq!(reflector.requests.borrow().len(), 3);
    assert!(privileged.calls.borrow().is_empty());
    fs::remove_dir_all(directory).expect("remove fixture");
}

#[test]
fn update_uses_global_fallback_when_regional_mirrors_fail() {
    // Given country and regional requests with only a global result available
    let directory = fixture("global-fallback");
    let target = directory.join("mirrorlist");
    fs::write(&target, "original").expect("write original mirrorlist");
    let reflector = FakeReflector {
        countries: "China\n".to_owned(),
        responses: RefCell::new(VecDeque::from([
            None,
            None,
            Some("Server = https://global.example/$repo/os/$arch\n".to_owned()),
        ])),
        requests: RefCell::new(Vec::new()),
    };
    let privileged = FakePrivilegedRunner {
        calls: RefCell::new(Vec::new()),
    };

    // When the mirror update flow exhausts country and regional results
    update("China", &directory, &reflector, &privileged)
        .execute()
        .expect("global mirror update");

    // Then it tries country, region, global and publishes the global result
    assert_eq!(
        *reflector.requests.borrow(),
        [
            Some("China".to_owned()),
            Some("China,Hong Kong,Taiwan,Japan,Singapore".to_owned()),
            None
        ]
    );
    assert_eq!(
        fs::read_to_string(&target).expect("read globally updated mirrorlist"),
        "Server = https://global.example/$repo/os/$arch\n"
    );
    fs::remove_dir_all(directory).expect("remove fixture");
}

#[test]
fn update_skips_unknown_country_and_uses_global_mirrors() {
    // Given an unrecognized country and a global mirror result
    let directory = fixture("unknown-country");
    let target = directory.join("mirrorlist");
    fs::write(&target, "original").expect("write original mirrorlist");
    let reflector = FakeReflector {
        countries: "China\nJapan\n".to_owned(),
        responses: RefCell::new(VecDeque::from([Some(
            "Server = https://global.example/$repo/os/$arch\n".to_owned(),
        )])),
        requests: RefCell::new(Vec::new()),
    };
    let privileged = FakePrivilegedRunner {
        calls: RefCell::new(Vec::new()),
    };

    // When the mirror update flow receives an unknown country
    update("Atlantis", &directory, &reflector, &privileged)
        .execute()
        .expect("global mirror update");

    // Then it skips the country request and uses the global result
    assert_eq!(*reflector.requests.borrow(), [None]);
    assert_eq!(
        fs::read_to_string(&target).expect("read globally updated mirrorlist"),
        "Server = https://global.example/$repo/os/$arch\n"
    );
    fs::remove_dir_all(directory).expect("remove fixture");
}
