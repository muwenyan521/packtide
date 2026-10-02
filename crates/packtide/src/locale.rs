#[derive(Clone, Copy)]
pub(crate) enum Lang {
    Zh,
    En,
}

pub(crate) fn current() -> Lang {
    resolve(
        std::env::var("PACKTIDE_UI_LANG").ok().as_deref(),
        std::env::var("LC_ALL").ok().as_deref(),
        std::env::var("LC_MESSAGES").ok().as_deref(),
        std::env::var("LANG").ok().as_deref(),
    )
}

fn resolve(
    explicit: Option<&str>,
    lc_all: Option<&str>,
    lc_messages: Option<&str>,
    lang: Option<&str>,
) -> Lang {
    if let Some(value) = explicit
        && !value.eq_ignore_ascii_case("auto")
        && let Some(lang) = parse_lang(value)
    {
        return lang;
    }
    [lc_all, lc_messages, lang]
        .into_iter()
        .flatten()
        .find_map(parse_lang)
        .unwrap_or(Lang::En)
}

fn parse_lang(value: &str) -> Option<Lang> {
    let language = value.split(['_', '-', '.', '@']).next()?;
    if language.eq_ignore_ascii_case("zh") {
        Some(Lang::Zh)
    } else if language.eq_ignore_ascii_case("en") {
        Some(Lang::En)
    } else {
        None
    }
}

pub(crate) fn text(lang: Lang, key: &str, replacements: &[(&str, &str)]) -> String {
    let source = match lang {
        Lang::Zh => include_str!("../locales/zh.txt"),
        Lang::En => include_str!("../locales/en.txt"),
    };
    let mut value = source
        .lines()
        .filter_map(|line| line.split_once('='))
        .find_map(|(name, value)| (name == key).then_some(value.to_owned()))
        .unwrap_or_else(|| format!("[missing:{key}]"));
    for (name, replacement) in replacements {
        value = value.replace(&format!("{{{name}}}"), replacement);
    }
    value
}

pub(crate) fn source_label(lang: Lang, source: system_tools_core::PackageSource) -> String {
    let key = match source {
        system_tools_core::PackageSource::Pacman => "source.pacman",
        system_tools_core::PackageSource::Aur => "source.aur",
        system_tools_core::PackageSource::Flatpak => "source.flatpak",
    };
    text(lang, key, &[])
}

pub(crate) fn backend_label(lang: Lang, backend: system_tools_core::BackendId) -> String {
    let key = match backend {
        system_tools_core::BackendId::Pacman => "backend.pacman",
        system_tools_core::BackendId::Paru
        | system_tools_core::BackendId::Yay
        | system_tools_core::BackendId::Flatpak => "backend.optional",
    };
    text(lang, key, &[])
}

pub(crate) fn scope_label(lang: Lang, scope: system_tools_core::PackageScope) -> String {
    let key = match scope {
        system_tools_core::PackageScope::System => "scope.system",
        system_tools_core::PackageScope::User => "scope.user",
    };
    text(lang, key, &[])
}

#[cfg(test)]
mod tests {
    use super::{Lang, parse_lang, resolve, source_label, text};

    #[test]
    fn normalizes_locale_region_and_encoding_suffixes() {
        assert!(matches!(parse_lang("zh_CN.UTF-8"), Some(Lang::Zh)));
        assert!(matches!(parse_lang("en-US"), Some(Lang::En)));
        assert!(parse_lang("fr_FR.UTF-8").is_none());
    }

    #[test]
    fn auto_uses_supported_environment_locale() {
        assert!(matches!(parse_lang("zh_CN.UTF-8"), Some(Lang::Zh)));
        assert!(matches!(parse_lang("en_US.UTF-8"), Some(Lang::En)));
        assert!(matches!(
            resolve(Some("en"), Some("zh_CN"), None, None),
            Lang::En
        ));
        assert!(matches!(
            resolve(Some("auto"), Some("zh_CN"), None, Some("en_US")),
            Lang::Zh
        ));
        assert!(matches!(
            resolve(None, None, Some("en_US"), Some("zh_CN")),
            Lang::En
        ));
    }

    #[test]
    fn localized_strings_apply_named_replacements() {
        assert_eq!(
            text(Lang::En, "picker.using", &[("helper", "paru")]),
            "Using paru"
        );
        assert_eq!(
            text(Lang::Zh, "picker.using", &[("helper", "paru")]),
            "使用 paru"
        );
        assert_eq!(
            text(Lang::En, "preview.unknown_kind", &[("kind", "bogus")]),
            "Unknown preview kind: bogus"
        );
        assert_eq!(
            text(Lang::Zh, "preview.missing_package", &[]),
            "预览需要软件包名称。"
        );
        assert!(text(Lang::Zh, "preview.failed", &[("error", "timeout")]).contains("timeout"));
        assert_eq!(
            source_label(Lang::Zh, system_tools_core::PackageSource::Pacman),
            "官方源"
        );
    }
}
