//! OPDS 1.2 katalogları (Atom beslemesi): gezinme, kitap girdileri, sayfalama
//! ve arama. Calibre Content Server ve Project Gutenberg bu biçimi kullanır.
//!
//! - Gezinme girdisi: `application/atom+xml` bağlantısı (alt katalog).
//! - Kitap girdisi: `rel` değeri `http://opds-spec.org/acquisition` ile başlayan
//!   bağlantılar; yalnızca okuyabildiğimiz biçimler (EPUB, PDF) alınır, satın
//!   alma (`…/acquisition/buy`) atlanır.
//! - Sayfalama `rel="next"`; arama `rel="search"` (OpenSearch şablonu ya da
//!   açıklama dosyası).
//! - Göreli adresler beslemenin adresine göre çözülür.

use quick_xml::events::{BytesStart, Event};

use crate::library::Format;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feed {
    pub title: String,
    pub entries: Vec<Entry>,
    pub next: Option<String>,
    pub search: Option<Search>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Search {
    /// `{searchTerms}` içeren adres.
    Template(String),
    /// OpenSearch açıklama dosyasının adresi; şablon oradan okunur.
    Description(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub title: String,
    pub authors: Vec<String>,
    pub summary: String,
    pub language: Option<String>,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// Alt katalog.
    Navigation(String),
    /// İndirilebilir biçimler, tercih sırasıyla (önce EPUB).
    Book(Vec<(Format, String)>),
    /// Kitap ama okuyabildiğimiz biçimde değil (ya da yalnızca satılık).
    Unavailable,
}

#[derive(Debug)]
pub struct OpdsError(pub String);

use crate::epub::attr;

/// Ad alanı önekinden bağımsız yerel ad.
fn local(e: &BytesStart) -> String {
    e.local_name().as_ref().to_string()
}

#[derive(Default)]
struct EntryBuilder {
    title: String,
    authors: Vec<String>,
    summary: String,
    /// Özetin alındığı öğe ("summary" ya da "content"); ilk görülen kullanılır.
    summary_from: Option<String>,
    language: Option<String>,
    navigation: Option<String>,
    books: Vec<(Format, String)>,
    acquisition: bool,
}

/// Bağlantının türünden okuyabildiğimiz biçim.
fn format_of(mime: &str) -> Option<Format> {
    let base = mime.split(';').next().unwrap_or("").trim();
    match base {
        "application/epub+zip" => Some(Format::Epub),
        "application/pdf" => Some(Format::Pdf),
        _ => None,
    }
}

/// `href`'i `base`'e göre çözer (mutlak, `//`, `/`, `?` ve göreli yollar).
/// HTTPS ile açılmış beslemeden gelen düz HTTP bağlantı HTTPS'e yükseltilir:
/// şifreli katalogdan şifresiz bağlantıya inilmez.
pub fn resolve(base: &str, href: &str) -> String {
    let out = join(base, href);
    match out.strip_prefix("http://") {
        Some(rest) if base.starts_with("https://") => format!("https://{rest}"),
        _ => out,
    }
}

fn join(base: &str, href: &str) -> String {
    let href = href.trim();
    if href.contains("://") {
        return href.to_string();
    }
    let scheme_end = base.find("://").map_or(0, |i| i + 3);
    let scheme = &base[..scheme_end.saturating_sub(3)];
    let host_end = base[scheme_end..]
        .find('/')
        .map_or(base.len(), |i| scheme_end + i);
    if let Some(rest) = href.strip_prefix("//") {
        return format!("{scheme}://{rest}");
    }
    if href.starts_with('/') {
        return format!("{}{href}", &base[..host_end]);
    }
    let without_query = base.split(['?', '#']).next().unwrap_or(base);
    if href.starts_with('?') {
        return format!("{without_query}{href}");
    }
    let dir_end = without_query
        .rfind('/')
        .filter(|&i| i >= host_end)
        .map_or(without_query.len(), |i| i + 1);
    let dir = if dir_end > without_query.len() || without_query.len() == host_end {
        format!("{without_query}/")
    } else {
        without_query[..dir_end].to_string()
    };
    // "./" ve "../" bölümleri.
    let (root, path) = dir.split_at(host_end);
    let mut parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut tail: Vec<&str> = href.split('/').collect();
    let last = tail.pop().unwrap_or("");
    for seg in tail {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    let mut out = root.to_string();
    for p in &parts {
        out.push('/');
        out.push_str(p);
    }
    out.push('/');
    out.push_str(last);
    out
}

/// Beslemeyi ayrıştırır; `url` beslemenin kendi adresidir (göreli bağlantılar için).
pub fn parse(xml: &[u8], url: &str) -> Result<Feed, OpdsError> {
    let text = String::from_utf8_lossy(xml);
    let mut reader = crate::epub::lenient_reader(&text);
    let mut feed = Feed {
        title: String::new(),
        entries: Vec::new(),
        next: None,
        search: None,
    };
    let mut entry: Option<EntryBuilder> = None;
    // Açık öğelerin yerel adları; metnin nereye ait olduğu buradan anlaşılır.
    let mut path: Vec<String> = Vec::new();
    let mut saw_feed = false;
    let mut buf = Vec::new();
    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|e| OpdsError(e.to_string()))?;
        match event {
            Event::Empty(e) if local(&e) == "link" => link(&e, url, &mut feed, entry.as_mut()),
            Event::Empty(_) => {}
            Event::Start(e) => {
                let name = local(&e);
                if name == "link" {
                    // Dolu <link>…</link>: kapanışı yığından çıkacağı için yığına girer.
                    link(&e, url, &mut feed, entry.as_mut());
                }
                if name == "feed" {
                    saw_feed = true;
                }
                if name == "entry" {
                    entry = Some(EntryBuilder::default());
                }
                if let Some(b) = entry.as_mut()
                    && path.last().map(String::as_str) == Some("entry")
                    && (name == "summary" || name == "content")
                {
                    b.summary_from.get_or_insert(name.clone());
                }
                if let Some(b) = entry.as_mut()
                    && name == "name"
                    && path.last().map(String::as_str) == Some("author")
                {
                    b.authors.push(String::new());
                }
                path.push(name);
            }
            Event::End(_) => {
                if path.pop().as_deref() == Some("entry")
                    && let Some(b) = entry.take()
                {
                    feed.entries.push(finish(b));
                }
            }
            Event::Text(t) => append_text(&path, &t.xml10_content(), &mut feed, entry.as_mut()),
            Event::CData(t) => append_text(&path, &t.xml10_content(), &mut feed, entry.as_mut()),
            Event::GeneralRef(r) => {
                let mut s = String::new();
                crate::epub::push_ref(&mut s, &r);
                append_text(&path, &s, &mut feed, entry.as_mut());
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    if !saw_feed {
        return Err(OpdsError("not an Atom feed".into()));
    }
    Ok(feed)
}

fn append_text(path: &[String], text: &str, feed: &mut Feed, entry: Option<&mut EntryBuilder>) {
    let Some(b) = entry else {
        if path.len() >= 2 && path[path.len() - 2] == "feed" && path[path.len() - 1] == "title" {
            feed.title.push_str(text);
        }
        return;
    };
    let Some(i) = path.iter().rposition(|p| p == "entry") else {
        return;
    };
    match &path[i + 1..] {
        [t] if t == "title" => b.title.push_str(text),
        [a, n] if a == "author" && n == "name" => {
            if let Some(last) = b.authors.last_mut() {
                last.push_str(text);
            }
        }
        [l] if l == "language" => b.language.get_or_insert_with(String::new).push_str(text),
        // XHTML içerikte metin iç içe öğelerdedir.
        [s, ..] if b.summary_from.as_deref() == Some(s.as_str()) => b.summary.push_str(text),
        _ => {}
    }
}

fn link(e: &BytesStart, base: &str, feed: &mut Feed, entry: Option<&mut EntryBuilder>) {
    let Some(href) = attr(e, "href") else {
        return;
    };
    let rel = attr(e, "rel").unwrap_or_default();
    let kind = attr(e, "type").unwrap_or_default();
    let href = resolve(base, &href);
    match entry {
        Some(b) => {
            if rel.starts_with("http://opds-spec.org/acquisition") {
                b.acquisition = true;
                if rel.ends_with("/buy") || rel.ends_with("/subscribe") {
                    return;
                }
                if let Some(f) = format_of(&kind)
                    && !b.books.iter().any(|(bf, _)| *bf == f)
                {
                    b.books.push((f, href));
                }
            } else if kind.starts_with("application/atom+xml")
                && !rel.contains("image")
                && rel != "alternate"
                && b.navigation.is_none()
            {
                b.navigation = Some(href);
            }
        }
        None => match rel.as_str() {
            "next" => feed.next = Some(href),
            "search" if kind.starts_with("application/opensearchdescription+xml") => {
                feed.search = Some(Search::Description(href));
            }
            "search" if href.contains("{searchTerms}") => {
                feed.search = Some(Search::Template(href));
            }
            _ => {}
        },
    }
}

fn finish(b: EntryBuilder) -> Entry {
    let mut books = b.books;
    // EPUB önce (yeniden akar), sonra PDF.
    books.sort_by_key(|(f, _)| *f != Format::Epub);
    let kind = if !books.is_empty() {
        EntryKind::Book(books)
    } else if b.acquisition {
        EntryKind::Unavailable
    } else if let Some(nav) = b.navigation {
        EntryKind::Navigation(nav)
    } else {
        EntryKind::Unavailable
    };
    Entry {
        title: b.title.split_whitespace().collect::<Vec<_>>().join(" "),
        authors: b
            .authors
            .into_iter()
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect(),
        summary: strip_tags(&b.summary)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        language: b.language.map(|l| l.trim().to_string()),
        kind,
    }
}

/// HTML özetinden etiketleri atar.
fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    crate::dict::render::entities(&out)
}

/// OpenSearch açıklamasından Atom sonuç şablonu (`{searchTerms}` içeren).
pub fn opensearch_template(xml: &[u8], url: &str) -> Option<String> {
    let text = String::from_utf8_lossy(xml);
    let mut reader = crate::epub::lenient_reader(&text);
    let mut buf = Vec::new();
    let mut fallback = None;
    loop {
        match reader.read_event_into(&mut buf).ok()? {
            Event::Start(e) | Event::Empty(e) if local(&e) == "Url" => {
                let template = attr(&e, "template")?;
                let kind = attr(&e, "type").unwrap_or_default();
                if kind.starts_with("application/atom+xml") {
                    return Some(resolve(url, &template));
                }
                fallback.get_or_insert(resolve(url, &template));
            }
            Event::Eof => return fallback,
            _ => {}
        }
        buf.clear();
    }
}

/// Arama şablonunu doldurur: `{searchTerms}` yüzde kodlanmış sorguyla; diğer
/// isteğe bağlı parametreler (`{startPage?}` vb.) boşaltılır.
pub fn search_url(template: &str, query: &str) -> String {
    let mut encoded = String::new();
    for b in query.trim().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char)
            }
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{b:02X}")),
        }
    }
    let mut out = template.replace("{searchTerms}", &encoded);
    while let Some(s) = out.find('{') {
        let Some(e) = out[s..].find('}') else { break };
        out.replace_range(s..s + e + 1, "");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Calibre Content Server'ın kök ve kitap beslemelerine benzer.
    const CALIBRE: &str = r#"<?xml version='1.0' encoding='utf-8'?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:dc="http://purl.org/dc/terms/" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>urn:calibre:main</id>
  <title>calibre Kütüphanesi</title>
  <link rel="search" type="application/opensearchdescription+xml" href="/opds/search"/>
  <link rel="next" type="application/atom+xml;profile=opds-catalog;kind=acquisition" href="/opds/navcatalog/4f?offset=25"/>
  <entry>
    <title>Yazarlara göre</title>
    <id>calibre:authors</id>
    <content type="text">Kitapları yazara göre listele</content>
    <link type="application/atom+xml;profile=opds-catalog;kind=navigation" href="/opds/navcatalog/4f" rel="subsection"/>
  </entry>
  <entry>
    <title>Dar Kapı</title>
    <author><name>André Gide</name></author>
    <dc:language>tur</dc:language>
    <content type="xhtml"><div xmlns="http://www.w3.org/1999/xhtml">Bir &lt;b&gt;aşk&lt;/b&gt; romanı.</div></content>
    <link type="application/x-mobipocket-ebook" href="/get/mobi/7/library" rel="http://opds-spec.org/acquisition"/>
    <link type="application/pdf" href="/get/pdf/7/library" rel="http://opds-spec.org/acquisition"/>
    <link type="application/epub+zip" href="/get/epub/7/library" rel="http://opds-spec.org/acquisition"/>
    <link type="image/jpeg" href="/get/cover/7/library" rel="http://opds-spec.org/image"/>
    <link type="application/atom+xml;type=entry" href="/opds/book/7" rel="alternate"/>
  </entry>
  <entry>
    <title>Satılık</title>
    <link type="application/epub+zip" href="buy" rel="http://opds-spec.org/acquisition/buy"/>
  </entry>
</feed>"#;

    #[test]
    fn calibre_feed() {
        let f = parse(CALIBRE.as_bytes(), "http://192.168.1.5:8080/opds").unwrap();
        assert_eq!(f.title, "calibre Kütüphanesi");
        assert_eq!(
            f.next.as_deref(),
            Some("http://192.168.1.5:8080/opds/navcatalog/4f?offset=25")
        );
        assert_eq!(
            f.search,
            Some(Search::Description(
                "http://192.168.1.5:8080/opds/search".into()
            ))
        );
        assert_eq!(f.entries.len(), 3);
        assert_eq!(
            f.entries[0].kind,
            EntryKind::Navigation("http://192.168.1.5:8080/opds/navcatalog/4f".into())
        );
        let book = &f.entries[1];
        assert_eq!(book.title, "Dar Kapı");
        assert_eq!(book.authors, vec!["André Gide"]);
        assert_eq!(book.language.as_deref(), Some("tur"));
        assert_eq!(
            book.kind,
            EntryKind::Book(vec![
                (
                    Format::Epub,
                    "http://192.168.1.5:8080/get/epub/7/library".into()
                ),
                (
                    Format::Pdf,
                    "http://192.168.1.5:8080/get/pdf/7/library".into()
                ),
            ])
        );
        assert_eq!(
            f.entries[2].kind,
            EntryKind::Unavailable,
            "yalnızca satılık"
        );
    }

    #[test]
    fn gutenberg_style_relative_links_and_html_summary() {
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom">
<title>Popüler</title>
<link rel="search" type="application/atom+xml" href="/ebooks/search.opds/?query={searchTerms}"/>
<entry><title>Kitap</title>
<summary type="html">&lt;p&gt;Özet &amp;amp; açıklama&lt;/p&gt;</summary>
<link type="application/epub+zip" rel="http://opds-spec.org/acquisition/open-access" href="../12345.epub3.images"/>
</entry></feed>"#;
        let f = parse(
            xml.as_bytes(),
            "https://www.gutenberg.org/ebooks/search.opds/?sort_order=downloads",
        )
        .unwrap();
        assert_eq!(
            f.search,
            Some(Search::Template(
                "https://www.gutenberg.org/ebooks/search.opds/?query={searchTerms}".into()
            ))
        );
        assert_eq!(f.entries[0].summary, "Özet & açıklama");
        assert_eq!(
            f.entries[0].kind,
            EntryKind::Book(vec![(
                Format::Epub,
                "https://www.gutenberg.org/ebooks/12345.epub3.images".into()
            )])
        );
    }

    #[test]
    fn urls_resolve_like_a_browser() {
        let base = "http://h:8080/a/b/c?x=1";
        for (href, want) in [
            ("https://o/z", "https://o/z"),
            ("//o/z", "http://o/z"),
            ("/z", "http://h:8080/z"),
            ("z", "http://h:8080/a/b/z"),
            ("./z", "http://h:8080/a/b/z"),
            ("../z", "http://h:8080/a/z"),
            ("?y=2", "http://h:8080/a/b/c?y=2"),
        ] {
            assert_eq!(resolve(base, href), want, "{href}");
        }
        assert_eq!(resolve("http://h", "z"), "http://h/z");
        assert_eq!(
            resolve("https://s/a", "http://m.s/q"),
            "https://m.s/q",
            "şifreli katalogdan şifresize inilmez"
        );
        assert_eq!(
            resolve("http://192.168.1.5/opds", "http://192.168.1.5/get"),
            "http://192.168.1.5/get"
        );
    }

    #[test]
    fn opensearch_and_query_encoding() {
        let desc = r#"<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">
<Url type="text/html" template="/search?q={searchTerms}"/>
<Url type="application/atom+xml" template="/opds/search/{searchTerms}?start={startPage?}"/>
</OpenSearchDescription>"#;
        let t = opensearch_template(desc.as_bytes(), "http://h/opds/search").unwrap();
        assert_eq!(t, "http://h/opds/search/{searchTerms}?start={startPage?}");
        assert_eq!(
            search_url(&t, "Sabahattin Ali"),
            "http://h/opds/search/Sabahattin+Ali?start="
        );
        assert_eq!(
            search_url("{searchTerms}", "çay&şeker"),
            "%C3%A7ay%26%C5%9Feker"
        );
    }

    #[test]
    fn non_feeds_and_broken_xml_are_errors() {
        assert!(parse(b"<html><body>giris</body></html>", "http://h/").is_err());
        // Yarıda kesilmiş besleme panik yaratmaz (hata ya da eksik sonuç).
        let _ = parse(b"<feed><entry><title>x</ti", "http://h/");
    }
}
