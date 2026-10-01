use crate::messages::{Lang, msg};
use quick_xml::Reader;
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesRef, Event};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Debug)]
pub(crate) struct NewsItem {
    pub(crate) title: String,
    pub(crate) date: String,
    pub(crate) link: String,
    pub(crate) urgent: bool,
}

pub(crate) fn news_failure_message(lang: Lang) -> &'static str {
    msg(lang, "news_fail")
}

pub(crate) fn parse_news(xml: &str, count: usize) -> Vec<NewsItem> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut items = Vec::new();
    let mut current: Option<NewsItem> = None;
    let mut field = String::new();
    let mut field_has_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_ascii_lowercase();
                if name == "item" || name == "entry" {
                    current = Some(NewsItem {
                        title: String::new(),
                        date: String::new(),
                        link: String::new(),
                        urgent: false,
                    });
                }
                field = name;
                field_has_text = false;
            }
            Ok(Event::Text(text)) => {
                if let Some(item) = current.as_mut() {
                    let value = text.decode().map(|v| v.into_owned()).unwrap_or_default();
                    if !value.trim().is_empty() {
                        field_has_text = true;
                        append_field(item, &field, &value);
                    }
                }
            }
            Ok(Event::GeneralRef(reference)) => {
                if let Some(item) = current.as_mut() {
                    let value = resolve_reference(&reference);
                    if !value.is_empty() {
                        field_has_text = true;
                        append_field(item, &field, &value);
                    }
                }
            }
            Ok(Event::CData(text)) => {
                if let Some(item) = current.as_mut() {
                    let value = text.decode().map(|v| v.into_owned()).unwrap_or_default();
                    append_field(item, &field, &value);
                }
            }
            Ok(Event::End(event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_ascii_lowercase();
                if field == name {
                    if field_has_text && let Some(item) = current.as_mut() {
                        trim_field(item, &name);
                    }
                    field.clear();
                    field_has_text = false;
                }
                if (name == "item" || name == "entry") && current.is_some() {
                    if let Some(item) = current.take() {
                        items.push(item);
                    }
                    if items.len() >= count {
                        break;
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    items
}

fn trim_field(item: &mut NewsItem, field: &str) {
    match field {
        "title" => item.title = item.title.trim().to_owned(),
        "pubdate" | "published" | "updated" => item.date = item.date.trim().to_owned(),
        "link" => item.link = item.link.trim().to_owned(),
        _ => {}
    }
}

fn append_field(item: &mut NewsItem, field: &str, value: &str) {
    match field {
        "title" => {
            item.title.push_str(value);
            let title = item.title.to_lowercase();
            item.urgent = ["intervention", "manual", "手动", "干预"]
                .iter()
                .any(|word| title.contains(word));
        }
        "pubdate" | "published" | "updated" => item.date.push_str(value),
        "link" => item.link.push_str(value),
        _ => {}
    }
}

fn resolve_reference(reference: &BytesRef<'_>) -> String {
    if let Ok(Some(character)) = reference.resolve_char_ref() {
        return character.to_string();
    }
    let Ok(name) = reference.decode() else {
        return String::new();
    };
    resolve_predefined_entity(name.as_ref())
        .map(str::to_owned)
        .unwrap_or_default()
}

pub(crate) fn fetch_news(source: &str, count: usize, lang: Lang) -> Option<Vec<NewsItem>> {
    let url = if source == "cn" {
        "https://www.archlinuxcn.org/category/news/feed/"
    } else {
        "https://archlinux.org/feeds/news/"
    };
    let agent = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(10)))
        .timeout_recv_body(Some(std::time::Duration::from_secs(20)))
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .new_agent();
    let response = agent.get(url).header("User-Agent", "systide/0.1").call();
    let Ok(response) = response else {
        println!("{}", news_failure_message(lang));
        return None;
    };
    let content_length = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if content_length.is_some_and(|size| size > 8 * 1024 * 1024) {
        println!("{}", news_failure_message(lang));
        return None;
    }
    let Ok(body) = response.into_body().read_to_string() else {
        println!("{}", news_failure_message(lang));
        return None;
    };
    if body.len() > 16 * 1024 * 1024 {
        println!("{}", news_failure_message(lang));
        return None;
    }
    Some(parse_news(&body, count))
}

pub(crate) fn print_news(lang: Lang, items: &[NewsItem]) {
    let plain = std::env::var_os("NO_COLOR").is_some();
    let title = msg(lang, "news_title").replace("{count}", &items.len().to_string());
    if plain {
        println!("\n{}\n", title);
    } else {
        println!("\n\x1b[1;33m{}\x1b[0m\n", title);
    }
    let width = std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|width: &usize| *width > 0)
        .unwrap_or(80);
    for item in items {
        print!("{}", render_news_item_with_width(item, lang, plain, width));
    }
}

fn render_news_item_with_width(item: &NewsItem, lang: Lang, plain: bool, width: usize) -> String {
    use std::fmt::Write as _;
    let date = if item.date.is_empty() {
        msg(lang, "news_no_date").to_owned()
    } else {
        truncate_display(&item.date, 16)
    };
    let title = truncate_display(
        &format!("{}{}", if item.urgent { "!!! " } else { "" }, item.title),
        width.saturating_sub(2),
    );
    let link_width = width.saturating_sub(2 + UnicodeWidthStr::width(date.as_str()) + 3);
    let link = truncate_display(&item.link, link_width);
    let mut rendered = String::new();
    if plain {
        writeln!(rendered, "{}", title).expect("write plain news title");
    } else {
        let color = if item.urgent { "1;31" } else { "2" };
        writeln!(rendered, "\x1b[{color}m{}\x1b[0m", title).expect("write news title");
    }
    if item.link.is_empty() {
        writeln!(rendered, "  {date}").expect("write news date");
    } else if plain {
        writeln!(rendered, "  {date} · {}", item.link).expect("write plain news link");
    } else {
        writeln!(
            rendered,
            "\x1b[2m  {date} · \x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\\x1b[0m",
            item.link, link
        )
        .expect("write OSC-8 news link");
    }
    rendered
}

fn truncate_display(value: &str, width: usize) -> String {
    if UnicodeWidthStr::width(value) <= width {
        return value.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + char_width > width.saturating_sub(3) {
            break;
        }
        out.push(ch);
        used += char_width;
    }
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::{NewsItem, render_news_item_with_width, truncate_display};
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn truncates_cjk_dates_by_display_width() {
        let value = truncate_display("2026 年 10 月 01 日 12:34", 16);
        assert!(UnicodeWidthStr::width(value.as_str()) <= 16);
        assert!(value.ends_with("..."));
    }

    #[test]
    fn renders_ordinary_news_as_dimmed_title_with_plain_link_fallback() {
        let item = NewsItem {
            title: "Update title".to_owned(),
            date: "2026-10-01".to_owned(),
            link: "https://example.test/news".to_owned(),
            urgent: false,
        };
        let rendered = render_news_item_with_width(&item, crate::messages::Lang::En, true, 80);
        assert!(rendered.contains("Update title\n  2026-10-01 · https://example.test/news"));
        assert!(!rendered.contains("\x1b["));
        assert!(!rendered.contains("\x1b]"));
    }

    #[test]
    fn renders_dimmed_ordinary_news_and_osc8_link() {
        let item = NewsItem {
            title: "Update title".to_owned(),
            date: "2026-10-01".to_owned(),
            link: "https://example.test/news".to_owned(),
            urgent: false,
        };
        let rendered = render_news_item_with_width(&item, crate::messages::Lang::En, false, 80);
        assert!(rendered.contains("\x1b[2mUpdate title\x1b[0m"));
        assert!(rendered.contains("\x1b]8;;https://example.test/news\x1b\\"));
    }

    #[test]
    fn narrow_news_item_preserves_full_plain_url_even_when_wide() {
        let item = NewsItem {
            title: "A very long title for a narrow terminal".to_owned(),
            date: "2026-10-01".to_owned(),
            link: "https://example.test/a-very-long-news-link".to_owned(),
            urgent: true,
        };
        let rendered = render_news_item_with_width(&item, crate::messages::Lang::En, true, 24);
        assert!(
            rendered
                .lines()
                .any(|line| UnicodeWidthStr::width(line) > 24)
        );
        assert!(rendered.contains("https://example.test/a-very-long-news-link"));
    }
}
