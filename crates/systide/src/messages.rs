#[derive(Clone, Copy)]
pub(crate) enum Lang {
    Zh,
    En,
}

pub(crate) fn msg(lang: Lang, key: &str) -> &'static str {
    let source = match lang {
        Lang::Zh => include_str!("../locales/zh.txt"),
        Lang::En => include_str!("../locales/en.txt"),
    };
    source
        .lines()
        .find_map(|line| {
            line.strip_prefix(key)
                .and_then(|line| line.strip_prefix('='))
        })
        .unwrap_or(match lang {
            Lang::Zh => "未定义的系统更新消息。",
            Lang::En => "Undefined system update message.",
        })
}

pub(crate) fn msg_with(lang: Lang, key: &str, replacements: &[(&str, String)]) -> String {
    let mut value = msg(lang, key).to_owned();
    for (name, replacement) in replacements {
        value = value.replace(&format!("{{{name}}}"), replacement);
    }
    value
}

pub(crate) fn source_label(lang: Lang, source: &str) -> &'static str {
    let key = match source {
        "pacman" => "source.pacman",
        "aur" => "source.aur",
        "flatpak" => "source.flatpak",
        _ => "backend.unsupported",
    };
    msg(lang, key)
}

pub(crate) fn print_intro(lang: Lang, native: system_tools_core::NativeBackend) {
    let prefix = match native {
        system_tools_core::NativeBackend::Pacman => "arch",
        system_tools_core::NativeBackend::Apt => "apt",
        system_tools_core::NativeBackend::Dnf => "dnf",
        system_tools_core::NativeBackend::Zypper => "zypper",
        system_tools_core::NativeBackend::Apk => "apk",
        system_tools_core::NativeBackend::Xbps => "xbps",
    };
    println!("\x1b[1;36m{}\x1b[0m", msg(lang, &format!("intro.{prefix}")));
    println!("{}", msg(lang, "desc"));
    for index in 1..=6 {
        println!(
            "  {index}. {}",
            msg(lang, &format!("step_{prefix}_{index}"))
        );
    }
}

pub(crate) fn log_info(lang: Lang, text: &str) {
    println!("\x1b[1;34m[{}]\x1b[0m {text}", msg(lang, "label_info"));
}

pub(crate) fn log_success(lang: Lang, text: &str) {
    println!("\x1b[1;32m[{}]\x1b[0m {text}", msg(lang, "label_success"));
}

pub(crate) fn log_warn(lang: Lang, text: &str) {
    println!("\x1b[1;33m[{}]\x1b[0m {text}", msg(lang, "label_warn"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apt_intro_is_not_arch_specific() {
        let native = system_tools_core::NativeBackend::Apt;
        let prefix = match native {
            system_tools_core::NativeBackend::Apt => "apt",
            _ => unreachable!(),
        };
        assert!(msg(Lang::En, &format!("intro.{prefix}")).contains("Debian/Ubuntu"));
        assert!(!msg(Lang::En, &format!("step_{prefix}_3")).contains("Pacman"));
    }
}
