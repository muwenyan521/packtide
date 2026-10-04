use anyhow::Result;
#[cfg(test)]
use system_tools_core::detect_native_backend;
use system_tools_core::{BackendId, NativeBackend, detect_native_backend_from_file};

use crate::messages::{Lang, log_info, msg};

pub(crate) fn perform_update(backend: BackendId, lang: Lang) -> Result<()> {
    crate::snapshot::create(lang);
    log_info(lang, msg(lang, "db_step"));
    crate::mirror::check_age(lang)?;
    crate::update::run(backend, lang)?;
    crate::finish::run(lang)
}

pub(crate) fn detect_manager(lang: Lang) -> Result<BackendId> {
    let native = detect_native_backend_from_file("/etc/os-release");
    backend_for_detection(native, lang)
}

fn backend_for_detection(
    native: Result<NativeBackend, system_tools_core::PlatformError>,
    lang: Lang,
) -> Result<BackendId> {
    let native = native.map_err(|error| {
        let key = match error {
            system_tools_core::PlatformError::MissingTool => "backend.missing_tool",
            system_tools_core::PlatformError::MalformedOsRelease
            | system_tools_core::PlatformError::UnsupportedDistribution => "backend.unsupported",
        };
        anyhow::anyhow!("{} ({})", msg(lang, key), msg(lang, "capability.upgrade"))
    })?;
    let resolver =
        system_tools_core::ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    let backend = native.backend_id(|command| resolver.resolve(command).is_some());
    Ok(backend)
}

#[cfg(test)]
fn detect_manager_from_input(
    os_release: &str,
    command_available: impl Fn(&std::ffi::OsStr) -> bool,
    lang: Lang,
) -> Result<BackendId> {
    let native = detect_native_backend(os_release, &command_available);
    let backend = backend_for_detection(native, lang)?;
    Ok(if matches!(backend, BackendId::Dnf4 | BackendId::Dnf5) {
        NativeBackend::Dnf.backend_id(command_available)
    } else {
        backend
    })
}

#[cfg(test)]
mod tests {
    use super::detect_manager_from_input;
    use crate::messages::{Lang, msg};
    use std::ffi::OsStr;
    use system_tools_core::BackendId;

    #[test]
    fn arch_uses_pacman_when_native_command_is_available() {
        let manager = detect_manager_from_input(
            "ID=arch\n",
            |command| command == OsStr::new("pacman"),
            Lang::En,
        )
        .expect("Arch should select pacman");
        assert_eq!(manager, BackendId::Pacman);
    }

    #[test]
    fn ubuntu_does_not_use_a_dnf_path_decoy_and_localizes_missing_tool() {
        let error = detect_manager_from_input(
            "ID=ubuntu\nID_LIKE=fedora\n",
            |command| command == OsStr::new("dnf"),
            Lang::Zh,
        )
        .expect_err("Ubuntu must require apt tools");
        let message = error.to_string();
        assert!(message.contains(msg(Lang::Zh, "backend.missing_tool")));
        assert!(message.contains(msg(Lang::Zh, "capability.upgrade")));
    }

    #[test]
    fn supported_non_arch_native_backend_is_selected() {
        let manager = detect_manager_from_input(
            "ID=fedora\n",
            |command| command == OsStr::new("dnf"),
            Lang::Zh,
        )
        .expect("Fedora should select dnf");
        assert_eq!(manager, BackendId::Dnf4);
    }

    #[test]
    fn dnf5_only_native_backend_is_selected() {
        let manager = detect_manager_from_input(
            "ID=fedora\n",
            |command| command == OsStr::new("dnf5"),
            Lang::En,
        )
        .expect("DNF5-only host should be supported");
        assert_eq!(manager, BackendId::Dnf5);
    }
}
