//! RSS 2.0 ve Atom beslemelerini okuma: besleme başlığı ve yazılar (kimlik,
//! başlık, bağlantı, tarih, HTML içerik). İçerik `content:encoded` /
//! `content`, yoksa `description` / `summary`'den gelir.

use quick_xml::events::Event;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    /// guid / id, yoksa bağlantı, o da yoksa başlık; okundu bilgisi buna bağlı.
    pub id: String,
    pub title: String,
    pub link: String,
    /// Beslemede yazdığı gibi (biçimler çok çeşitli; çevrilmez).
    pub date: String,
    pub html: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Channel {
    pub title: String,
    pub items: Vec<Item>,
}

/// Beslemeyi ayrıştırır; RSS ya da Atom değilse `None`.
pub fn parse(xml: &str) -> Option<Channel> {
    let mut r = crate::epub::lenient_reader(xml);
    let mut channel = Channel::default();
    let mut is_feed = false;
    let mut item: Option<Item> = None;
    // Toplanan alan ve metni; içerik ile özet ayrı tutulur.
    let mut field: Option<String> = None;
    let mut buf = String::new();
    let mut summary = String::new();
    while let Ok(ev) = r.read_event() {
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev, Event::Empty(_));
                let name = e.local_name().as_ref().to_string();
                match name.as_str() {
                    "rss" | "feed" | "RDF" => is_feed = true,
                    "item" | "entry" => {
                        item = Some(Item::default());
                        summary.clear();
                    }
                    // Atom bağlantısı: `rel` yoksa ya da "alternate"se.
                    "link" if attr(e, "href").is_some() => {
                        let rel = attr(e, "rel").unwrap_or_default();
                        if let Some(it) = item.as_mut()
                            && (rel.is_empty() || rel == "alternate")
                            && it.link.is_empty()
                        {
                            it.link = attr(e, "href").unwrap_or_default();
                        }
                    }
                    "title" | "link" | "guid" | "id" | "pubDate" | "updated" | "published"
                    | "date" | "description" | "summary" | "encoded" | "content"
                        if !empty =>
                    {
                        field = Some(name);
                        buf.clear();
                    }
                    _ => {}
                }
            }
            Event::Text(t) if field.is_some() => buf.push_str(&t.xml10_content()),
            Event::CData(t) if field.is_some() => {
                buf.push_str(&t.into_inner());
            }
            Event::GeneralRef(g) if field.is_some() => crate::epub::push_ref(&mut buf, &g),
            Event::End(e) => {
                let name = e.local_name().as_ref().to_string();
                if field.as_deref() == Some(name.as_str()) {
                    let text = std::mem::take(&mut buf);
                    let clean = || crate::epub::collapse_ws(&text);
                    match (item.as_mut(), name.as_str()) {
                        (None, "title") if channel.title.is_empty() => channel.title = clean(),
                        (Some(it), "title") => it.title = clean(),
                        (Some(it), "link") if it.link.is_empty() => it.link = clean(),
                        (Some(it), "guid" | "id") => it.id = clean(),
                        (Some(it), "pubDate" | "updated" | "published" | "date")
                            if it.date.is_empty() =>
                        {
                            it.date = clean();
                        }
                        (Some(it), "encoded" | "content") => it.html = text.trim().to_string(),
                        (Some(_), "description" | "summary") => summary = text.trim().to_string(),
                        _ => {}
                    }
                    field = None;
                }
                if (name == "item" || name == "entry")
                    && let Some(mut it) = item.take()
                {
                    if it.html.is_empty() {
                        it.html = std::mem::take(&mut summary);
                    }
                    if it.id.is_empty() {
                        it.id = if it.link.is_empty() {
                            it.title.clone()
                        } else {
                            it.link.clone()
                        };
                    }
                    channel.items.push(it);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    is_feed.then_some(channel)
}

fn attr(e: &quick_xml::events::BytesStart, name: &str) -> Option<String> {
    crate::epub::attr(e, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_items_prefer_full_content() {
        let xml = r#"<?xml version="1.0"?>
<rss version="2.0" xmlns:content="http://purl.org/rss/1.0/modules/content/"><channel>
<title>Günlük &amp; Notlar</title><link>https://ornek.org</link>
<item><title>Birinci yazı</title><link>https://ornek.org/1</link><guid>g1</guid>
<pubDate>Wed, 28 Oct 2026 10:00:00 +0300</pubDate>
<description>Kısa özet</description>
<content:encoded><![CDATA[<p>Uzun <b>içerik</b></p>]]></content:encoded></item>
<item><title>İkinci</title><link>https://ornek.org/2</link><description>&lt;p&gt;Yalnız özet&lt;/p&gt;</description></item>
</channel></rss>"#;
        let c = parse(xml).unwrap();
        assert_eq!(c.title, "Günlük & Notlar");
        assert_eq!(c.items.len(), 2);
        assert_eq!(c.items[0].id, "g1");
        assert_eq!(c.items[0].html, "<p>Uzun <b>içerik</b></p>");
        assert!(c.items[0].date.starts_with("Wed, 28 Oct"));
        assert_eq!(c.items[1].id, "https://ornek.org/2", "guid yoksa bağlantı");
        assert_eq!(c.items[1].html, "<p>Yalnız özet</p>");
    }

    #[test]
    fn atom_entries_and_non_feeds() {
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Atom</title>
<entry><title>Yazı</title><id>urn:1</id><updated>2026-10-28T10:00:00Z</updated>
<link rel="edit" href="https://ornek.org/duzenle"/><link href="https://ornek.org/yazi"/>
<summary>Özet</summary><content type="html">&lt;p&gt;Gövde&lt;/p&gt;</content></entry></feed>"#;
        let c = parse(xml).unwrap();
        assert_eq!(c.title, "Atom");
        let e = &c.items[0];
        assert_eq!(
            (e.id.as_str(), e.link.as_str()),
            ("urn:1", "https://ornek.org/yazi")
        );
        assert_eq!(e.html, "<p>Gövde</p>");
        assert_eq!(parse("<html><body>sayfa</body></html>"), None);
    }
}
