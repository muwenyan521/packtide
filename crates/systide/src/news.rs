use crate::messages::{Lang, msg};
use quick_xml::Reader;
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesRef, Event};

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
    use std::fmt::Write as _;

    println!(
        "\n\x1b[1;33m{}\x1b[0m\n",
        if matches!(lang, Lang::Zh) {
            "最近的系统新闻："
        } else {
            "Recent system news:"
        }
    );
    let mut rendered = String::new();
    for item in items {
        rendered.clear();
        let color = if item.urgent { "1;31" } else { "1;32" };
        let date = if item.date.is_empty() {
            "No date"
        } else {
            &item.date[..item
                .date
                .char_indices()
                .nth(16)
                .map_or(item.date.len(), |(i, _)| i)]
        };
        write!(rendered, "\x1b[{color}m").expect("write news color");
        if !item.link.is_empty() {
            write!(rendered, "\x1b]8;;{}\x1b\\", item.link).expect("write news link");
        }
        write!(
            rendered,
            "[{date}] {}{}",
            if item.urgent { "!!! " } else { "" },
            item.title
        )
        .expect("write news line");
        if !item.link.is_empty() {
            rendered.push_str("\x1b]8;;\x1b\\");
        }
        rendered.push_str("\x1b[0m\n");
        print!("{rendered}");
    }
}
