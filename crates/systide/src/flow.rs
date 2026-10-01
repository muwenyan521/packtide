use crate::messages::{Lang, msg};
use anyhow::{Result, bail};
use std::env;
use std::io::{self, BufRead, Write};

pub(crate) fn language(value: &str) -> Result<Lang> {
    match value {
        "zh" => Ok(Lang::Zh),
        "en" => Ok(Lang::En),
        "auto" => Ok(language_from_locale(select_locale(
            env::var("LC_ALL").ok().as_deref(),
            env::var("LC_MESSAGES").ok().as_deref(),
            env::var("LANG").ok().as_deref(),
        ))),
        _ => {
            let lang = if value.starts_with("zh") {
                Lang::Zh
            } else {
                Lang::En
            };
            bail!("{}", msg(lang, "invalid_ui_lang"));
        }
    }
}

pub(crate) fn language_from_locale(locale: Option<&str>) -> Lang {
    if locale.is_some_and(|value| value.contains("zh_CN")) {
        Lang::Zh
    } else {
        Lang::En
    }
}

pub(crate) fn select_locale<'a>(
    lc_all: Option<&'a str>,
    lc_messages: Option<&'a str>,
    lang: Option<&'a str>,
) -> Option<&'a str> {
    lc_all.or(lc_messages).or(lang)
}

pub(crate) fn confirm_start(lang: Lang, manager: &str) -> Result<bool> {
    print!("{}", msg(lang, "confirm").replace("%s", manager));
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    Ok(answer.trim().is_empty() || answer.trim_start().starts_with(['y', 'Y']))
}

pub(crate) fn confirm_force(lang: Lang) -> Result<bool> {
    print!("{}", msg(lang, "force_confirm"));
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    Ok(answer.trim_start().starts_with(['y', 'Y']))
}

#[cfg(test)]
mod tests {
    use super::{Lang, language_from_locale, select_locale};

    #[test]
    fn locale_detection_prefers_explicit_locale_value() {
        assert!(matches!(
            language_from_locale(Some("zh_CN.UTF-8")),
            Lang::Zh
        ));
        assert!(matches!(
            language_from_locale(Some("en_US.UTF-8")),
            Lang::En
        ));
        assert!(matches!(language_from_locale(None), Lang::En));
        assert_eq!(
            select_locale(Some("en_US"), Some("zh_CN"), Some("zh_CN")),
            Some("en_US")
        );
        assert_eq!(
            select_locale(None, Some("zh_CN"), Some("en_US")),
            Some("zh_CN")
        );
    }
}
