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

pub(crate) fn print_intro(lang: Lang) {
    println!("\x1b[1;36m{}\x1b[0m", msg(lang, "intro"));
    println!("{}", msg(lang, "desc"));
    for index in 1..=6 {
        println!("  {index}. {}", msg(lang, &format!("step_{index}")));
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

pub(crate) fn log_error(lang: Lang, text: &str) {
    println!("\x1b[1;31m[{}]\x1b[0m {text}", msg(lang, "label_error"));
}
