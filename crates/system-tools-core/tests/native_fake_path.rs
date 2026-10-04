use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use system_tools_core::{
    BackendId, BuiltinBackend, ExecutableResolver, NativePackageKey, PackageBackend,
    PackageIdentity, PackageKind, PackageScope, ReadOperation, WriteOperation,
};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "system-tools-core-native-fake-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir(&path).expect("create fake PATH");
        Self(path)
    }

    fn command(&self, name: &str) {
        let body = r#"
case "$0" in
  *apt-cache) cat "$FIXTURE_ROOT/apt/catalog.deb822" ;;
  *dnf5) printf '%s\n' '[{"name":"bash","version":"5.2","arch":"x86_64"}]' ;;
  *zypper) cat "$FIXTURE_ROOT/zypper/search.xml" ;;
  *apk) cat "$FIXTURE_ROOT/apk/search.tsv" ;;
  *xbps-query) cat "$FIXTURE_ROOT/xbps/search.tsv" ;;
  *) exit 0 ;;
esac
"#;
        let path = self.0.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}")).expect("write fake command");
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .expect("make fake command executable");
    }

    fn path(&self) -> std::ffi::OsString {
        std::env::join_paths([self.0.as_path(), Path::new("/usr/bin"), Path::new("/bin")])
            .expect("build fake PATH")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn identity(backend: BackendId, name: &str) -> PackageIdentity {
    PackageIdentity::new(
        backend,
        PackageKind::System,
        PackageScope::System,
        NativePackageKey::new(name).expect("non-empty fixture key"),
    )
}

#[test]
fn native_backend_fake_path_catalog_and_install_plans() {
    if std::env::var_os("NATIVE_FAKE_CHILD").is_some() {
        run_child();
        return;
    }

    let fixture = Fixture::new();
    for command in [
        "apt-cache",
        "apt-get",
        "dpkg-query",
        "dnf5",
        "zypper",
        "apk",
        "xbps-query",
        "xbps-install",
        "xbps-remove",
    ] {
        fixture.command(command);
    }
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .arg("--exact")
        .arg("native_backend_fake_path_catalog_and_install_plans")
        .arg("--nocapture")
        .env("NATIVE_FAKE_CHILD", "1")
        .env(
            "FIXTURE_ROOT",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/package-managers/fixtures"),
        )
        .env("PATH", fixture.path())
        .output()
        .expect("run native fake-path child");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_child() {
    let path = std::env::var_os("PATH").expect("child PATH");
    let resolver = ExecutableResolver::from_path(Some(&path));
    let cases = [
        (BackendId::Apt, "bash"),
        (BackendId::Dnf5, "bash"),
        (BackendId::Zypper, "lib&foo"),
        (BackendId::Apk, "libfoo-bar"),
        (BackendId::Xbps, "lib-foo"),
    ];
    for (backend, expected) in cases {
        let provider = BuiltinBackend::new(backend);
        let result = provider
            .read(ReadOperation::Catalog)
            .unwrap_or_else(|error| panic!("{backend:?} catalog failed: {error}"));
        assert!(
            result
                .packages
                .iter()
                .any(|package| package.native_key.as_str() == expected),
            "{backend:?} catalog did not contain {expected}: {:?}",
            result.packages
        );
        let package = identity(backend, expected);
        let plan = provider
            .write_with_resolver(
                WriteOperation::Install {
                    packages: vec![package],
                },
                &resolver,
            )
            .unwrap_or_else(|error| panic!("{backend:?} install plan failed: {error}"));
        assert_eq!(plan.backend, backend);
        assert!(
            plan.command.args.iter().any(|arg| arg == expected),
            "{backend:?} install argv missing package: {:?}",
            plan.command.args
        );
    }
}
