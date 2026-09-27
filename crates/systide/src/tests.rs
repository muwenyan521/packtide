use super::messages::Lang;
use super::news::{news_failure_message, parse_news};

#[test]
fn news_fetch_failure_message_uses_selected_language() {
    assert_eq!(
        news_failure_message(Lang::Zh),
        "获取新闻失败（网络或源错误）"
    );
    assert_eq!(news_failure_message(Lang::En), "Failed to fetch news");
}

#[test]
fn parses_rss_items_with_limit() {
    let xml = r#"<rss><channel><item><title>First</title><pubDate>2026-09-24T10:00:00Z</pubDate><link>https://example.test/1</link></item><item><title>Second</title></item></channel></rss>"#;
    let items = parse_news(xml, 1);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "First");
    assert_eq!(items[0].link, "https://example.test/1");
}

#[test]
fn parses_rss_cdata_entities_and_urgent_titles() {
    let xml = r#"<rss><channel><item><title><![CDATA[Manual intervention &amp; action]]></title><pubDate>2026-09-24</pubDate><link>https://example.test/urgent</link></item></channel></rss>"#;
    let items = parse_news(xml, 10);
    assert_eq!(items.len(), 1);
    assert!(items[0].urgent);
    assert!(items[0].title.contains("Manual intervention"));
    assert_eq!(items[0].link, "https://example.test/urgent");
}

#[test]
fn parses_xml_named_and_numeric_references_in_rss_fields() {
    let xml = r#"<rss><channel><item><title>Security &amp; stability &#x2014; update</title><link>https://example.test/feed?a=1&amp;b=2</link></item></channel></rss>"#;
    let items = parse_news(xml, 10);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Security & stability \u{2014} update");
    assert_eq!(items[0].link, "https://example.test/feed?a=1&b=2");
}

#[test]
fn preserves_text_spacing_around_xml_references_and_cdata() {
    let xml = r#"<rss><channel><item><title>  Security &amp; stability &#x2014; update  </title></item><item><title><![CDATA[ padded ]]></title></item></channel></rss>"#;
    let items = parse_news(xml, 10);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "Security & stability \u{2014} update");
    assert_eq!(items[1].title, " padded ");
}
