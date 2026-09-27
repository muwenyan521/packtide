#[derive(Clone, Copy)]
pub(crate) enum Lang {
    Zh,
    En,
}

pub(crate) fn current() -> Lang {
    match std::env::var("PACKTIDE_UI_LANG").ok().as_deref() {
        Some("zh") => Lang::Zh,
        Some("en") => Lang::En,
        _ => std::env::var("LANG")
            .ok()
            .filter(|value| value.to_ascii_lowercase().contains("zh"))
            .map_or(Lang::En, |_| Lang::Zh),
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
