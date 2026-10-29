//! FB2, TXT ve HTML'yi açılırken bellekte EPUB'a çevirir; böylece okuma,
//! içindekiler, dipnot, kapak, seri, vurgu ve arama EPUB'dakiyle aynı çalışır.
//! Dosyaya hiçbir şey yazılmaz.
//!
//! - FB2 (ve `.fb2.zip`): üst düzey her `section` bir bölüm; `title` başlık,
//!   `epigraph`/`cite` alıntı, `poem` dizeler, `binary` görseller (base64),
//!   `coverpage` kapak, `sequence` seri, notlar gövdesi dipnot bölümü.
//! - TXT: boş satırla ayrılmış paragraflar (boş satır azsa her satır bir
//!   paragraf); "Bölüm 3", "Chapter 3" gibi kısa satırlar bölüm başı.
//! - HTML: tek bölüm; başlık `<title>`'dan. Yanındaki görseller okunmaz.
//!
//! UTF-8 olmayan metin Windows-1254 (Türkçe) sayılır; FB2'de bildirilen
//! Windows-1251 (Kiril) de tanınır.

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use quick_xml::events::Event;

use super::{EpubError, MAX_ENTRY_BYTES, attr, lenient_reader, push_ref};

/// TXT bölümleri en çok bu kadar bayt olur (büyük dosya tek bölüm olmasın).
const TXT_CHAPTER_BYTES: usize = 100_000;
/// Çevrilecek dosya için üst sınır.
const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;

/// Dosya uzantısı EPUB değilse çevrilmiş EPUB; EPUB ise `None`.
pub(super) fn to_epub(path: &Path) -> Result<Option<Vec<u8>>, EpubError> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let kind = if name.ends_with(".fb2") || name.ends_with(".fb2.zip") {
        "fb2"
    } else if name.ends_with(".txt") {
        "txt"
    } else if name.ends_with(".html") || name.ends_with(".htm") || name.ends_with(".xhtml") {
        "html"
    } else {
        return Ok(None);
    };
    let size = std::fs::metadata(path)?.len();
    if size > MAX_SOURCE_BYTES {
        return Err(EpubError::TooLarge(name));
    }
    let mut bytes = std::fs::read(path)?;
    if name.ends_with(".zip") {
        bytes = first_fb2_in_zip(bytes)?;
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().trim_end_matches(".fb2").to_string())
        .unwrap_or_default();
    let book = match kind {
        "fb2" => fb2(&decode(&bytes, true))?,
        "txt" => txt(&decode(&bytes, false), &stem),
        _ => html(&decode(&bytes, false), &stem),
    };
    Ok(Some(book.build()))
}

fn first_fb2_in_zip(bytes: Vec<u8>) -> Result<Vec<u8>, EpubError> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| EpubError::NotZip(e.to_string()))?;
    let name = archive
        .file_names()
        .flatten()
        .find(|n| n.to_lowercase().ends_with(".fb2"))
        .map(|n| n.into_owned())
        .ok_or_else(|| EpubError::MissingEntry("*.fb2".into()))?;
    let entry = archive
        .by_name(&name)
        .map_err(|_| EpubError::MissingEntry(name.clone()))?;
    let mut out = Vec::new();
    entry.take(MAX_SOURCE_BYTES + 1).read_to_end(&mut out)?;
    if out.len() as u64 > MAX_SOURCE_BYTES {
        return Err(EpubError::TooLarge(name));
    }
    Ok(out)
}

// --- Kodlamalar ---

/// Windows-1251'in 0x80–0xBF arası (0xC0–0xFF: А–я sırayla).
const CP1251_HIGH: [char; 64] = [
    'Ђ', 'Ѓ', '‚', 'ѓ', '„', '…', '†', '‡', '€', '‰', 'Љ', '‹', 'Њ', 'Ќ', 'Ћ', 'Џ', //
    'ђ', '‘', '’', '“', '”', '•', '–', '—', '\u{98}', '™', 'љ', '›', 'њ', 'ќ', 'ћ', 'џ', //
    '\u{a0}', 'Ў', 'ў', 'Ј', '¤', 'Ґ', '¦', '§', 'Ё', '©', 'Є', '«', '¬', '\u{ad}', '®',
    'Ї', //
    '°', '±', 'І', 'і', 'ґ', 'µ', '¶', '·', 'ё', '№', 'є', '»', 'ј', 'Ѕ', 'ѕ', 'ї',
];

/// Windows-1254'ün Latin-1'den ayrılan yerleri: 0x80–0x9F ve altı Türkçe harf.
fn cp1254(b: u8) -> char {
    const LOW: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', '\u{8e}',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        '\u{9e}', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => LOW[(b - 0x80) as usize],
        0xD0 => 'Ğ',
        0xDD => 'İ',
        0xDE => 'Ş',
        0xF0 => 'ğ',
        0xFD => 'ı',
        0xFE => 'ş',
        _ => b as char,
    }
}

fn cp1251(b: u8) -> char {
    match b {
        0x00..=0x7F => b as char,
        0x80..=0xBF => CP1251_HIGH[(b - 0x80) as usize],
        _ => char::from_u32(0x0410 + (b - 0xC0) as u32).unwrap_or('?'),
    }
}

/// Baytları metne çevirir: UTF-8 (BOM'lu ya da değil), değilse (FB2'de
/// bildirilen 1251 dışında) Windows-1254.
fn decode(bytes: &[u8], xml: bool) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]).to_lowercase();
    let cyrillic = xml && (head.contains("1251") || head.contains("cp-1251"));
    bytes
        .iter()
        .map(|&b| if cyrillic { cp1251(b) } else { cp1254(b) })
        .collect()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Standart base64 (boşluklar atlanır); bozuksa `None`.
fn base64(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in text
        .bytes()
        .filter(|c| !c.is_ascii_whitespace() && *c != b'=')
    {
        acc = (acc << 6) | value(c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

// --- Bellekteki EPUB ---

#[derive(Default)]
struct Converted {
    title: String,
    authors: Vec<String>,
    language: Option<String>,
    identifier: Option<String>,
    series: Option<String>,
    series_index: Option<String>,
    /// (içindekilerdeki başlık, XHTML gövdesi); başlıksız bölüm içindekilerde yok.
    chapters: Vec<(Option<String>, String)>,
    /// (ad, MIME türü, baytlar); `images/<ad>` olarak.
    images: Vec<(String, String, Vec<u8>)>,
    cover: Option<String>,
}

fn chapter_file(i: usize) -> String {
    format!("c{i}.xhtml")
}

impl Converted {
    fn build(self) -> Vec<u8> {
        let mut manifest = String::new();
        let mut spine = String::new();
        let mut nav = String::new();
        for (i, (title, _)) in self.chapters.iter().enumerate() {
            let file = chapter_file(i);
            manifest.push_str(&format!(
                r#"<item id="c{i}" href="{file}" media-type="application/xhtml+xml"/>"#
            ));
            spine.push_str(&format!(r#"<itemref idref="c{i}"/>"#));
            if let Some(t) = title {
                nav.push_str(&format!(r#"<li><a href="{file}">{}</a></li>"#, escape(t)));
            }
        }
        for (n, (name, mime, _)) in self.images.iter().enumerate() {
            let cover = if self.cover.as_deref() == Some(name) {
                r#" properties="cover-image""#
            } else {
                ""
            };
            manifest.push_str(&format!(
                r#"<item id="i{n}" href="images/{}" media-type="{}"{cover}/>"#,
                escape(name),
                escape(mime)
            ));
        }
        let mut meta = format!("<dc:title>{}</dc:title>", escape(&self.title));
        for a in &self.authors {
            meta.push_str(&format!("<dc:creator>{}</dc:creator>", escape(a)));
        }
        if let Some(l) = &self.language {
            meta.push_str(&format!("<dc:language>{}</dc:language>", escape(l)));
        }
        if let Some(id) = &self.identifier {
            meta.push_str(&format!("<dc:identifier>{}</dc:identifier>", escape(id)));
        }
        if let Some(s) = &self.series {
            meta.push_str(&format!(
                r#"<meta name="calibre:series" content="{}"/>"#,
                escape(s)
            ));
            if let Some(i) = &self.series_index {
                meta.push_str(&format!(
                    r#"<meta name="calibre:series_index" content="{}"/>"#,
                    escape(i)
                ));
            }
        }
        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">{meta}</metadata>
<manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>{manifest}</manifest>
<spine>{spine}</spine>
</package>"#
        );
        let nav = format!(
            r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol>{nav}</ol></nav></body></html>"#
        );
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let mut put = |name: &str, body: &[u8]| {
            // Bellekteki yazıcıya yazmak yalnızca bellek bitince başarısız olur.
            if w.start_file(name, opts).is_ok() {
                let _ = w.write_all(body);
            }
        };
        put("mimetype", b"application/epub+zip");
        put(
            "META-INF/container.xml",
            br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
        );
        put("content.opf", opf.as_bytes());
        put("nav.xhtml", nav.as_bytes());
        for (i, (_, body)) in self.chapters.iter().enumerate() {
            let doc = format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>{body}</body></html>"#
            );
            put(&chapter_file(i), doc.as_bytes());
        }
        for (name, _, bytes) in &self.images {
            put(&format!("images/{name}"), bytes);
        }
        w.finish().map(|c| c.into_inner()).unwrap_or_default()
    }
}

// --- TXT ---

/// "Bölüm 3", "BİRİNCİ KISIM", "Chapter IV" gibi kısa başlık satırı mı?
fn is_heading(line: &str) -> bool {
    let l = line.trim();
    if l.is_empty() || l.chars().count() > 60 || l.ends_with(['.', ',', ';', ':']) {
        return false;
    }
    let lower: String = l.chars().map(crate::collate::turkish_lower).collect();
    [
        "bölüm",
        "kısım",
        "chapter",
        "part ",
        "kapitel",
        "chapitre",
        "capítulo",
        "capitolo",
    ]
    .iter()
    .any(|w| lower.starts_with(w) || lower.ends_with(w.trim()))
}

fn txt(text: &str, stem: &str) -> Converted {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.lines().collect();
    let blanks = lines.iter().filter(|l| l.trim().is_empty()).count();
    // Boş satır azsa her satır ayrı paragraftır.
    let by_blank = blanks * 20 >= lines.len();
    let mut paragraphs: Vec<String> = Vec::new();
    let mut cur = String::new();
    for line in &lines {
        let l = line.trim();
        if l.is_empty() || !by_blank || is_heading(l) {
            if !cur.is_empty() {
                paragraphs.push(std::mem::take(&mut cur));
            }
            if !l.is_empty() && (!by_blank || is_heading(l)) {
                paragraphs.push(l.to_string());
            }
            continue;
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(l);
    }
    if !cur.is_empty() {
        paragraphs.push(cur);
    }
    let mut book = Converted {
        title: stem.to_string(),
        ..Converted::default()
    };
    let mut body = String::new();
    let mut title: Option<String> = None;
    for p in paragraphs {
        if is_heading(&p) || body.len() >= TXT_CHAPTER_BYTES {
            if !body.is_empty() {
                book.chapters
                    .push((title.take(), std::mem::take(&mut body)));
            }
            if is_heading(&p) {
                body.push_str(&format!("<h2>{}</h2>", escape(&p)));
                title = Some(p);
                continue;
            }
        }
        body.push_str(&format!("<p>{}</p>", escape(&p)));
    }
    if !body.is_empty() || book.chapters.is_empty() {
        book.chapters.push((title, body));
    }
    book
}

// --- HTML ---

fn html(text: &str, stem: &str) -> Converted {
    let lower = text.to_lowercase();
    let title = lower
        .find("<title>")
        .and_then(|a| {
            let start = a + "<title>".len();
            let end = start + lower[start..].find("</title>")?;
            text.get(start..end)
        })
        .map(super::collapse_ws)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| stem.to_string());
    // Gövdenin içi olduğu gibi kullanılır; okuyucu bozuk HTML'ye hoşgörülü.
    let body = match (lower.find("<body"), lower.rfind("</body>")) {
        (Some(a), Some(b)) if a < b => {
            let start = a + lower[a..].find('>').map_or(0, |i| i + 1);
            &text[start.min(b)..b]
        }
        _ => text,
    };
    Converted {
        title,
        chapters: vec![(None, body.to_string())],
        ..Converted::default()
    }
}

// --- FB2 ---

fn fb2(text: &str) -> Result<Converted, EpubError> {
    let mut r = lenient_reader(text);
    let mut book = Converted::default();
    // Açık öğeler (yerel adlarıyla).
    let mut stack: Vec<String> = Vec::new();
    let mut body = String::new();
    let mut chapter_title: Option<String> = None;
    let mut collecting_title = false;
    let mut title_text = String::new();
    let mut in_notes = false;
    let mut notes = String::new();
    // Bölümüne göre kimlikler (iç bağlantılar için) ve bağlantılar sonra çözülür.
    let mut ids: HashMap<String, usize> = HashMap::new();
    let mut author = String::new();
    let mut binary: Option<(String, String)> = None;
    let mut binary_text = String::new();
    let mut text_buf = String::new();
    let mut cover_href: Option<String> = None;
    let mut first_body_done = false;
    let section_depth = |stack: &[String]| stack.iter().filter(|s| *s == "section").count();

    loop {
        let ev = r.read_event().map_err(|e| EpubError::Xml {
            entry: "fb2".into(),
            detail: e.to_string(),
        })?;
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev, Event::Empty(_));
                let name = e.local_name().as_ref().to_string();
                let in_desc = stack.iter().any(|s| s == "description");
                let out = if in_notes { &mut notes } else { &mut body };
                match name.as_str() {
                    "body" if !empty => {
                        let named = attr(e, "name").is_some();
                        in_notes = first_body_done || named;
                        first_body_done = true;
                    }
                    "sequence" if in_desc && book.series.is_none() => {
                        book.series = attr(e, "name").filter(|n| !n.trim().is_empty());
                        book.series_index = attr(e, "number");
                    }
                    "image" if in_desc => {
                        if stack.iter().any(|s| s == "coverpage") {
                            cover_href = attr(e, "href").map(|h| h.trim_start_matches('#').into());
                        }
                    }
                    "binary" if !empty => {
                        binary = Some((
                            attr(e, "id").unwrap_or_default(),
                            attr(e, "content-type").unwrap_or_else(|| "image/jpeg".into()),
                        ));
                        binary_text.clear();
                    }
                    _ if in_desc => text_buf.clear(),
                    "section" if !empty => {
                        if !in_notes && section_depth(&stack) == 0 && !body.trim().is_empty() {
                            book.chapters
                                .push((chapter_title.take(), std::mem::take(&mut body)));
                        }
                        let out = if in_notes { &mut notes } else { &mut body };
                        match attr(e, "id") {
                            Some(id) => {
                                out.push_str(&format!(r#"<div id="{}">"#, escape(&id)));
                                if !in_notes {
                                    ids.insert(id, book.chapters.len());
                                }
                            }
                            None => out.push_str("<div>"),
                        }
                    }
                    "title"
                        if !empty
                            && stack.last().is_some_and(|s| s == "section" || s == "body") =>
                    {
                        let level = (section_depth(&stack)).clamp(1, 6);
                        out.push_str(&format!("<h{level}>"));
                        collecting_title = !in_notes && section_depth(&stack) == 1;
                        title_text.clear();
                    }
                    "p" | "v" | "subtitle" | "text-author" if !empty => {
                        if stack.iter().any(|s| s == "title") {
                            if !out.ends_with('>') {
                                out.push(' ');
                            }
                            if collecting_title && !title_text.is_empty() {
                                title_text.push(' ');
                            }
                        } else {
                            out.push_str(match name.as_str() {
                                "subtitle" => "<p><strong>",
                                "text-author" => "<p><em>",
                                _ => "<p>",
                            });
                        }
                    }
                    "emphasis" if !empty => out.push_str("<em>"),
                    "strong" if !empty => out.push_str("<strong>"),
                    "sup" | "sub" if !empty => out.push_str(&format!("<{name}>")),
                    "epigraph" | "cite" | "annotation" if !empty => out.push_str("<blockquote>"),
                    "poem" | "stanza" if !empty => out.push_str("<div>"),
                    "table" | "tr" | "td" | "th" if !empty => out.push_str(&format!("<{name}>")),
                    "image" => {
                        if let Some(h) = attr(e, "href") {
                            let src = escape(h.trim_start_matches('#'));
                            out.push_str(&format!(r#"<img src="images/{src}" alt=""/>"#));
                        }
                    }
                    "a" if !empty => {
                        let href = attr(e, "href").unwrap_or_default();
                        let note = attr(e, "type").as_deref() == Some("note");
                        let target = match href.strip_prefix('#') {
                            // Bölümü sonradan belli olur: \0kimlik\0.
                            Some(id) => format!("\u{0}{}\u{0}", escape(id)),
                            None => escape(&href),
                        };
                        let kind = if note { r#" epub:type="noteref""# } else { "" };
                        out.push_str(&format!(r#"<a href="{target}"{kind}>"#));
                    }
                    _ => {}
                }
                if !empty {
                    stack.push(name);
                }
            }
            Event::End(e) => {
                let name = e.local_name().as_ref().to_string();
                if let Some(i) = stack.iter().rposition(|s| *s == name) {
                    stack.truncate(i);
                }
                let in_desc = stack.iter().any(|s| s == "description");
                let out = if in_notes { &mut notes } else { &mut body };
                match name.as_str() {
                    "book-title" if in_desc => book.title = super::collapse_ws(&text_buf),
                    "lang" if in_desc && book.language.is_none() => {
                        book.language = Some(text_buf.trim().to_string());
                    }
                    "id" if stack.last().is_some_and(|s| s == "document-info") => {
                        book.identifier = Some(text_buf.trim().to_string());
                    }
                    "first-name" | "middle-name" | "last-name" | "nickname"
                        if stack.last().is_some_and(|s| s == "author")
                            && stack.iter().any(|s| s == "title-info") =>
                    {
                        let part = super::collapse_ws(&text_buf);
                        if !part.is_empty() {
                            if !author.is_empty() {
                                author.push(' ');
                            }
                            author.push_str(&part);
                        }
                    }
                    "author" if stack.last().is_some_and(|s| s == "title-info") => {
                        if !author.is_empty() {
                            book.authors.push(std::mem::take(&mut author));
                        }
                    }
                    "binary" => {
                        if let Some((id, mime)) = binary.take()
                            && let Some(bytes) = base64(&binary_text)
                            && (bytes.len() as u64) <= MAX_ENTRY_BYTES
                        {
                            book.images.push((id, mime, bytes));
                        }
                    }
                    "body" => in_notes = false,
                    "section" => out.push_str("</div>"),
                    "title" if stack.last().is_some_and(|s| s == "section" || s == "body") => {
                        let level = section_depth(&stack).clamp(1, 6);
                        out.push_str(&format!("</h{level}>"));
                        if collecting_title {
                            chapter_title = Some(super::collapse_ws(&title_text));
                            collecting_title = false;
                        }
                    }
                    "p" | "v" | "subtitle" | "text-author" => {
                        if !stack.iter().any(|s| s == "title") {
                            out.push_str(match name.as_str() {
                                "subtitle" => "</strong></p>",
                                "text-author" => "</em></p>",
                                _ => "</p>",
                            });
                        }
                    }
                    "emphasis" => out.push_str("</em>"),
                    "strong" => out.push_str("</strong>"),
                    "sup" | "sub" | "table" | "tr" | "td" | "th" => {
                        out.push_str(&format!("</{name}>"));
                    }
                    "epigraph" | "cite" | "annotation" if !in_desc => out.push_str("</blockquote>"),
                    "poem" | "stanza" => out.push_str("</div>"),
                    "a" => out.push_str("</a>"),
                    _ => {}
                }
            }
            Event::Text(t) => {
                let s = t.xml10_content();
                if binary.is_some() {
                    binary_text.push_str(&s);
                } else if stack.iter().any(|s| s == "description") {
                    text_buf.push_str(&s);
                } else if stack.iter().any(|s| s == "body") {
                    let out = if in_notes { &mut notes } else { &mut body };
                    out.push_str(&escape(&s));
                    if collecting_title {
                        title_text.push_str(&s);
                    }
                }
            }
            Event::GeneralRef(g) => {
                let mut s = String::new();
                push_ref(&mut s, &g);
                if stack.iter().any(|s| s == "description") {
                    text_buf.push_str(&s);
                } else if stack.iter().any(|s| s == "body") {
                    let out = if in_notes { &mut notes } else { &mut body };
                    out.push_str(&escape(&s));
                    if collecting_title {
                        title_text.push_str(&s);
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !body.trim().is_empty() || book.chapters.is_empty() {
        book.chapters.push((chapter_title, body));
    }
    let notes_chapter = book.chapters.len();
    if !notes.trim().is_empty() {
        book.chapters.push((None, notes));
    }
    // İç bağlantılar: notlar bölümünde değilse kimliğin bulunduğu bölüm.
    for (_, body) in &mut book.chapters {
        let mut out = String::with_capacity(body.len());
        let mut parts = body.split('\u{0}');
        out.push_str(parts.next().unwrap_or_default());
        while let (Some(id), Some(rest)) = (parts.next(), parts.next()) {
            let file = chapter_file(ids.get(id).copied().unwrap_or(notes_chapter));
            out.push_str(&format!("{file}#{id}"));
            out.push_str(rest);
        }
        *body = out;
    }
    if book.title.is_empty() {
        book.title = "FB2".into();
    }
    book.cover = cover_href.filter(|c| book.images.iter().any(|(n, _, _)| n == c));
    Ok(book)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turkish_and_cyrillic_code_pages_and_base64() {
        assert_eq!(decode(b"a\xfd\xf0\xfe\xdd", false), "aığşİ");
        assert_eq!(decode("ağaç".as_bytes(), false), "ağaç");
        let xml = b"<?xml version=\"1.0\" encoding=\"windows-1251\"?>\xcf\xf0\xe8";
        assert!(decode(xml, true).ends_with("При"));
        assert_eq!(base64("TGVn\nZW50aWE=").unwrap(), b"Legentia");
        assert_eq!(base64("!!"), None);
    }

    #[test]
    fn txt_paragraphs_and_chapter_headings() {
        let t = "Bölüm 1\n\nBirinci satır\ndevamı.\n\nİkinci paragraf.\n\nBÖLÜM 2\n\nSon.\n";
        let b = txt(t, "kitap");
        assert_eq!(b.title, "kitap");
        assert_eq!(b.chapters.len(), 2);
        assert_eq!(b.chapters[0].0.as_deref(), Some("Bölüm 1"));
        assert!(b.chapters[0].1.contains("<p>Birinci satır devamı.</p>"));
        assert_eq!(b.chapters[1].0.as_deref(), Some("BÖLÜM 2"));
        // Boş satırsız dosyada her satır bir paragraf.
        let b = txt("bir\niki\nüç", "x");
        assert!(b.chapters[0].1.contains("<p>bir</p><p>iki</p><p>üç</p>"));
    }

    fn open(name: &str, bytes: &[u8]) -> super::super::Book {
        let dir = crate::storage::temp_store("donustur").dir().to_path_buf();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        super::super::Book::open(&path).unwrap()
    }

    const FB2: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
 <description><title-info>
  <genre>sf</genre>
  <author><first-name>Isaac</first-name><last-name>Asimov</last-name></author>
  <book-title>Vakıf &amp; İmparatorluk</book-title>
  <coverpage><image l:href="#kapak.png"/></coverpage>
  <lang>tr</lang>
  <sequence name="Vakıf" number="2"/>
 </title-info><document-info><id>fb2-0001</id></document-info></description>
 <body>
  <title><p>Vakıf</p></title>
  <section id="s1"><title><p>Birinci</p><p>Kısım</p></title>
   <epigraph><p>Söz.</p><text-author>Biri</text-author></epigraph>
   <p>Metin<a l:href="#n1" type="note">1</a> ve <emphasis>vurgu</emphasis>.</p>
   <section><title><p>Alt</p></title><p>İç <a l:href="#s2">bağ</a>.</p></section>
  </section>
  <section id="s2"><title><p>İkinci</p></title><poem><stanza><v>dize</v></stanza></poem>
   <image l:href="#kapak.png"/></section>
 </body>
 <body name="notes"><section id="n1"><title><p>1</p></title><p>Dipnot.</p></section></body>
 <binary id="kapak.png" content-type="image/png">KAPAK</binary>
</FictionBook>"##;

    fn fb2_sample() -> String {
        let png = crate::epub::testutil::png(4, 6, 50);
        let b64 = {
            const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut s = String::new();
            for c in png.chunks(3) {
                let n = c
                    .iter()
                    .enumerate()
                    .fold(0u32, |a, (i, &b)| a | (b as u32) << (16 - 8 * i));
                for i in 0..4 {
                    s.push(if i <= c.len() {
                        T[(n >> (18 - 6 * i) & 63) as usize] as char
                    } else {
                        '='
                    });
                }
            }
            s
        };
        FB2.replace("KAPAK", &b64)
    }

    #[test]
    fn fb2_opens_as_a_book_with_toc_cover_series_and_notes() {
        let mut book = open("vakif.fb2", fb2_sample().as_bytes());
        assert_eq!(book.title.as_deref(), Some("Vakıf & İmparatorluk"));
        assert_eq!(book.authors, ["Isaac Asimov"]);
        assert_eq!(book.identifier.as_deref(), Some("fb2-0001"));
        assert_eq!(book.language.as_deref(), Some("tr"));
        assert_eq!(
            (book.series.as_deref(), book.series_index),
            (Some("Vakıf"), Some(2.0))
        );
        assert_eq!(book.cover.as_deref(), Some("images/kapak.png"));
        assert!(book.image_bytes("images/kapak.png").is_some());
        let toc: Vec<&str> = book.toc.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(toc, ["Birinci Kısım", "İkinci"]);
        // Gövde başlığı, iki bölüm, notlar.
        assert_eq!(book.spine.len(), 4);
        let first = book.chapter(1).unwrap();
        let texts: Vec<&str> = first.iter().map(|b| b.text.as_str()).collect();
        assert!(texts.contains(&"Metin1 ve vurgu."), "{texts:?}");
        let link = first
            .iter()
            .flat_map(|b| &b.links)
            .find(|l| l.note)
            .unwrap();
        assert_eq!(link.href, "c3.xhtml#n1");
        let inner = first
            .iter()
            .flat_map(|b| &b.links)
            .find(|l| !l.note)
            .unwrap();
        assert_eq!(inner.href, "c2.xhtml#s2");
        let second = book.chapter(2).unwrap();
        assert!(second.iter().any(|b| b.text == "dize"));
        assert!(second.iter().any(|b| b.image.is_some()));
        // Sıkıştırılmış FB2 de açılır.
        let zipped = crate::epub::testutil::zip_of(&[("vakif.fb2", &fb2_sample())]);
        assert_eq!(
            open("vakif.fb2.zip", &zipped).title.as_deref(),
            Some("Vakıf & İmparatorluk")
        );
    }

    #[test]
    fn txt_and_html_open_as_books() {
        let mut book = open(
            "Çalıkuşu.txt",
            b"B\xd6L\xdcM 1\n\nFeride \xe7ok g\xfczel.\n",
        );
        assert_eq!(book.title.as_deref(), Some("Çalıkuşu"));
        let blocks = book.chapter(0).unwrap();
        assert_eq!(blocks[1].text, "Feride çok güzel.");
        let mut book = open(
            "sayfa.html",
            "<html><head><title>Bir Sayfa</title></head><body><h1>Başlık</h1><p>Ağaç &amp; gölge</p></body></html>".as_bytes(),
        );
        assert_eq!(book.title.as_deref(), Some("Bir Sayfa"));
        let blocks = book.chapter(0).unwrap();
        assert_eq!(blocks[1].text, "Ağaç & gölge");
    }
}
