use anyhow::Result;
#[cfg(test)]
use system_tools_core::detect_native_backend;
use system_tools_core::{
    BackendId, BuiltinBackend, CapabilitySet, NativeBackend, PackageBackend,
    detect_native_backend_from_file,
};

use crate::messages::{Lang, backend_label, log_info, msg};

pub(crate) fn perform_update(manager: &str, lang: Lang) -> Result<()> {
    crate::snapshot::create(lang);
    log_info(lang, msg(lang, "db_step"));
    crate::mirror::check_age(lang)?;
    crate::update::run(manager, lang)?;
    crate::finish::run(lang)
}

pub(crate) fn detect_manager(lang: Lang) -> Result<&'static str> {
    let native = detect_native_backend_from_file("/etc/os-release");
    manager_for_detection(native, lang)
}

fn manager_for_detection(
    native: Result<NativeBackend, system_tools_core::PlatformError>,
    lang: Lang,
) -> Result<&'static str> {
    let native = native.map_err(|error| {
        let key = match error {
            system_tools_core::PlatformError::MissingTool => "backend.missing_tool",
            system_tools_core::PlatformError::MalformedOsRelease
            | system_tools_core::PlatformError::UnsupportedDistribution => "backend.unsupported",
        };
        anyhow::anyhow!("{} ({})", msg(lang, key), msg(lang, "capability.upgrade"))
    })?;
    let (backend, manager) = match native {
        NativeBackend::Pacman => (BackendId::Pacman, "pacman"),
        NativeBackend::Apt
        | NativeBackend::Dnf
        | NativeBackend::Zypper
        | NativeBackend::Apk
        | NativeBackend::Xbps => {
            return Err(anyhow::anyhow!(
                "{} ({})",
                msg(lang, "backend.unsupported"),
                backend_label(lang, native)
            ));
        }
    };
    if !BuiltinBackend::new(backend)
        .capabilities()
        .contains(CapabilitySet::SYSTEM_UPGRADE)
    {
        return Err(anyhow::anyhow!("{}", msg(lang, "backend.unsupported")));
    }
    Ok(manager)
}

#[cfg(test)]
fn detect_manager_from_input(
    os_release: &str,
    command_available: impl Fn(&std::ffi::OsStr) -> bool,
    lang: Lang,
) -> Result<&'static str> {
    manager_for_detection(detect_native_backend(os_release, command_available), lang)
}

#[cfg(test)]
mod tests {
    use super::detect_manager_from_input;
    use crate::messages::{Lang, msg};
    use std::ffi::OsStr;

    #[test]
    fn arch_uses_pacman_when_native_command_is_available() {
        let manager = detect_manager_from_input(
            "ID=arch\n",
            |command| command == OsStr::new("pacman"),
            Lang::En,
        )
        .expect("Arch should select pacman");
        assert_eq!(manager, "pacman");
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
    fn supported_non_arch_native_backend_stays_explicitly_unsupported() {
        let error = detect_manager_from_input(
            "ID=fedora\n",
            |command| command == OsStr::new("dnf"),
            Lang::Zh,
        )
        .expect_err("Fedora is detected but unsupported by systide");
        let message = error.to_string();
        assert!(message.contains(msg(Lang::Zh, "backend.unsupported")));
        assert!(message.contains(msg(Lang::Zh, "backend.dnf")));
    }
}
