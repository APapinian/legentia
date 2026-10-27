use quick_xml::escape::resolve_html5_entity;
use quick_xml::events::{BytesRef, Event};
use unicode_normalization::UnicodeNormalization;

use super::{EpubError, attr, lenient_reader};
use crate::text::{Span, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BlockKind {
    Paragraph,
    /// 1–6, `<h1>`–`<h6>`.
    Heading(u8),
}

/// Bir paragraf ya da başlık. `spans` metnin tamamını sırayla kaplar ve
/// satır içi biçimi (kalın, italik, bağlantı) taşır. CSS okunmaz.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Block {
    pub kind: BlockKind,
    pub text: String,
    pub spans: Vec<Span>,
}

/// Blok başlatan/bitiren öğeler. Diğerleri (span, b, i, a…) satır içidir.
const BLOCK_TAGS: &[&str] = &[
    "p",
    "div",
    "li",
    "blockquote",
    "br",
    "tr",
    "dt",
    "dd",
    "section",
    "article",
    "header",
    "footer",
    "figcaption",
    "pre",
    "hr",
    "table",
    "ul",
    "ol",
    "aside",
    "body",
];
/// İçeriği hiç gösterilmeyen öğeler.
const SKIP_TAGS: &[&str] = &["head", "script", "style", "svg", "math"];
const BOLD_TAGS: &[&str] = &["b", "strong"];
const ITALIC_TAGS: &[&str] = &["i", "em", "cite", "var", "dfn"];

fn heading_level(name: &str) -> Option<u8> {
    match name {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

/// Açık biçim etiketlerinin sayısı; iç içe `<b><b>` doğru kapanır.
#[derive(Default)]
struct Open {
    bold: u32,
    italic: u32,
    /// Her açık `<a>` için: `href` taşıyor mu.
    links: Vec<bool>,
}

impl Open {
    fn style(&self) -> Style {
        Style {
            bold: self.bold > 0,
            italic: self.italic > 0,
            link: self.links.iter().any(|&l| l),
        }
    }
}

/// XHTML belgesini metin bloklarına ayırır. Kapanmamış ya da uyuşmayan
/// etiketlere hoşgörülüdür; okunamayan XML hata olarak döner.
pub(super) fn blocks(text: &str, path: &str) -> Result<Vec<Block>, EpubError> {
    let mut reader = lenient_reader(text);

    let mut out = Vec::new();
    let mut pieces: Vec<(String, Style)> = Vec::new();
    let mut heading: Option<u8> = None;
    let mut skip = 0u32;
    let mut open = Open::default();

    loop {
        let event = reader.read_event().map_err(|e| EpubError::Xml {
            entry: path.to_string(),
            detail: e.to_string(),
        })?;
        match event {
            Event::Start(e) => {
                let name = e.local_name().as_ref().to_ascii_lowercase();
                let name = name.as_str();
                if SKIP_TAGS.contains(&name) {
                    skip += 1;
                } else if let Some(level) = heading_level(name) {
                    flush(&mut out, &mut pieces, heading);
                    heading = Some(level);
                } else if BLOCK_TAGS.contains(&name) {
                    flush(&mut out, &mut pieces, heading);
                } else if BOLD_TAGS.contains(&name) {
                    open.bold += 1;
                } else if ITALIC_TAGS.contains(&name) {
                    open.italic += 1;
                } else if name == "a" {
                    open.links.push(attr(&e, "href").is_some());
                }
            }
            Event::Empty(e) => {
                let name = e.local_name().as_ref().to_ascii_lowercase();
                if BLOCK_TAGS.contains(&name.as_str()) {
                    flush(&mut out, &mut pieces, heading);
                }
            }
            Event::End(e) => {
                let name = e.local_name().as_ref().to_ascii_lowercase();
                let name = name.as_str();
                if SKIP_TAGS.contains(&name) {
                    skip = skip.saturating_sub(1);
                } else if heading_level(name).is_some() {
                    flush(&mut out, &mut pieces, heading);
                    heading = None;
                } else if BLOCK_TAGS.contains(&name) {
                    flush(&mut out, &mut pieces, heading);
                } else if BOLD_TAGS.contains(&name) {
                    open.bold = open.bold.saturating_sub(1);
                } else if ITALIC_TAGS.contains(&name) {
                    open.italic = open.italic.saturating_sub(1);
                } else if name == "a" {
                    open.links.pop();
                }
            }
            Event::Text(t) if skip == 0 => push(&mut pieces, &t.xml10_content(), open.style()),
            Event::CData(t) if skip == 0 => push(&mut pieces, &t.xml10_content(), open.style()),
            Event::GeneralRef(r) if skip == 0 => {
                let mut s = String::new();
                push_ref(&mut s, &r);
                push(&mut pieces, &s, open.style());
            }
            Event::Eof => break,
            _ => {}
        }
    }
    flush(&mut out, &mut pieces, heading);
    Ok(out)
}

fn push(pieces: &mut Vec<(String, Style)>, text: &str, style: Style) {
    match pieces.last_mut() {
        Some((s, st)) if *st == style => s.push_str(text),
        _ => pieces.push((text.to_string(), style)),
    }
}

/// Parçaları tek metinde birleştirir: boşlukları tek boşluğa indirir, NFC'ye
/// çevirir, yumuşak tire ve sıfır genişlikli boşluğu atar (heceleme bizde).
fn flush(out: &mut Vec<Block>, pieces: &mut Vec<(String, Style)>, heading: Option<u8>) {
    let mut text = String::new();
    let mut spans: Vec<Span> = Vec::new();
    // Bekleyen boşluk: bir sonraki görünür karakterden önce, ilk boşluğun biçimiyle yazılır.
    let mut pending: Option<Style> = None;
    for (piece, style) in pieces.drain(..) {
        for c in piece
            .nfc()
            .filter(|c| !matches!(c, '\u{00AD}' | '\u{200B}'))
        {
            if c.is_whitespace() {
                if !text.is_empty() && pending.is_none() {
                    pending = Some(style);
                }
                continue;
            }
            if let Some(space_style) = pending.take() {
                append(&mut text, &mut spans, ' ', space_style);
            }
            append(&mut text, &mut spans, c, style);
        }
    }
    if text.is_empty() {
        return;
    }
    let kind = heading.map_or(BlockKind::Paragraph, BlockKind::Heading);
    out.push(Block { kind, text, spans });
}

fn append(text: &mut String, spans: &mut Vec<Span>, c: char, style: Style) {
    let start = text.len();
    text.push(c);
    match spans.last_mut() {
        Some(last) if last.style == style => last.end = text.len(),
        _ => spans.push(Span {
            start,
            end: text.len(),
            style,
        }),
    }
}

/// `&amp;`, `&#305;`, `&nbsp;` gibi başvuruları çözer; bilinmeyenleri olduğu gibi bırakır.
pub(super) fn push_ref(buf: &mut String, r: &BytesRef) {
    if let Ok(Some(c)) = r.resolve_char_ref() {
        buf.push(c);
    } else if let Some(s) = resolve_html5_entity(r) {
        buf.push_str(s);
    } else {
        buf.push('&');
        buf.push_str(r);
        buf.push(';');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(html: &str) -> Vec<(BlockKind, String)> {
        blocks(html, "t.xhtml")
            .unwrap()
            .into_iter()
            .map(|b| (b.kind, b.text))
            .collect()
    }

    fn styled(html: &str) -> Vec<(String, Style)> {
        let b = blocks(html, "t.xhtml").unwrap().remove(0);
        b.spans
            .iter()
            .map(|s| (b.text[s.start..s.end].to_string(), s.style))
            .collect()
    }

    #[test]
    fn splits_paragraphs_and_headings() {
        let html = "<html><head><title>X</title><style>p{}</style></head><body>\
            <h2>Başlık <i>bir</i></h2><p>İlk\n   paragraf.</p><div>İkinci<br/>satır</div></body></html>";
        assert_eq!(
            texts(html),
            vec![
                (BlockKind::Heading(2), "Başlık bir".into()),
                (BlockKind::Paragraph, "İlk paragraf.".into()),
                (BlockKind::Paragraph, "İkinci".into()),
                (BlockKind::Paragraph, "satır".into()),
            ]
        );
    }

    #[test]
    fn resolves_entities() {
        let html = "<p>A&amp;B &#305;&#x130; &nbsp;&laquo;söz&raquo; &bilinmeyen;</p>";
        assert_eq!(texts(html)[0].1, "A&B ıİ «söz» &bilinmeyen;");
    }

    #[test]
    fn inline_tags_do_not_split_words() {
        assert_eq!(texts("<p>gö<b>z</b>lük</p>")[0].1, "gözlük");
    }

    #[test]
    fn strips_soft_hyphens() {
        assert_eq!(texts("<p>ke\u{00AD}li\u{00AD}me</p>")[0].1, "kelime");
    }

    #[test]
    fn composes_decomposed_turkish_letters() {
        // Gerçek bir kitaptan: "BİRİNCİ BEŞİNCİ" ayrışık yazılmış.
        let html = "<h1>BI\u{0307}RI\u{0307}NCI\u{0307} BES\u{0327}I\u{0307}NCI\u{0307}</h1>";
        assert_eq!(texts(html)[0].1, "BİRİNCİ BEŞİNCİ");
    }

    #[test]
    fn keeps_bold_italic_and_links_as_spans() {
        let b = Style {
            bold: true,
            ..Style::default()
        };
        let i = Style {
            italic: true,
            ..Style::default()
        };
        let bi = Style {
            bold: true,
            italic: true,
            ..Style::default()
        };
        let link = Style {
            link: true,
            ..Style::default()
        };
        assert_eq!(
            styled(
                "<p>Düz <b>kalın <i>ikisi</i></b> <em>eğik</em> <a href='#n1'>dipnot</a><a id='x'>çapa</a></p>"
            ),
            vec![
                ("Düz ".into(), Style::default()),
                ("kalın ".into(), b),
                ("ikisi".into(), bi),
                (" ".into(), Style::default()),
                ("eğik".into(), i),
                (" ".into(), Style::default()),
                ("dipnot".into(), link),
                ("çapa".into(), Style::default()),
            ]
        );
    }

    #[test]
    fn spans_cover_text_without_gaps() {
        let blocks = blocks("<p>  <i> baştan </i> <b>  boşluk </b>  son </p>", "t.xhtml").unwrap();
        let b = &blocks[0];
        assert_eq!(b.text, "baştan boşluk son");
        assert_eq!(b.spans.first().unwrap().start, 0);
        assert_eq!(b.spans.last().unwrap().end, b.text.len());
        for w in b.spans.windows(2) {
            assert_eq!(w[0].end, w[1].start);
        }
    }
}
