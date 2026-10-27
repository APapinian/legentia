use quick_xml::escape::resolve_html5_entity;
use quick_xml::events::{BytesRef, Event};

use super::{EpubError, collapse_ws, lenient_reader};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    /// 1–6, `<h1>`–`<h6>`.
    Heading(u8),
}

/// Bir paragraf ya da başlık; içindeki biçimlendirme (kalın, italik) M3'e kadar atılır.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub text: String,
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

/// XHTML belgesini metin bloklarına ayırır. Kapanmamış ya da uyuşmayan
/// etiketlere hoşgörülüdür; okunamayan XML hata olarak döner.
pub(super) fn blocks(text: &str, path: &str) -> Result<Vec<Block>, EpubError> {
    let mut reader = lenient_reader(text);

    let mut out = Vec::new();
    let mut buf = String::new();
    let mut heading: Option<u8> = None;
    let mut skip = 0u32;

    loop {
        let event = reader.read_event().map_err(|e| EpubError::Xml {
            entry: path.to_string(),
            detail: e.to_string(),
        })?;
        match event {
            Event::Start(e) => {
                let name = e.local_name().as_ref().to_ascii_lowercase();
                if SKIP_TAGS.contains(&name.as_str()) {
                    skip += 1;
                } else if let Some(level) = heading_level(&name) {
                    flush(&mut out, &mut buf, heading);
                    heading = Some(level);
                } else if BLOCK_TAGS.contains(&name.as_str()) {
                    flush(&mut out, &mut buf, heading);
                }
            }
            Event::Empty(e) => {
                let name = e.local_name().as_ref().to_ascii_lowercase();
                if BLOCK_TAGS.contains(&name.as_str()) {
                    flush(&mut out, &mut buf, heading);
                }
            }
            Event::End(e) => {
                let name = e.local_name().as_ref().to_ascii_lowercase();
                if SKIP_TAGS.contains(&name.as_str()) {
                    skip = skip.saturating_sub(1);
                } else if heading_level(&name).is_some() {
                    flush(&mut out, &mut buf, heading);
                    heading = None;
                } else if BLOCK_TAGS.contains(&name.as_str()) {
                    flush(&mut out, &mut buf, heading);
                }
            }
            Event::Text(t) if skip == 0 => buf.push_str(&t.xml10_content()),
            Event::CData(t) if skip == 0 => buf.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if skip == 0 => push_ref(&mut buf, &r),
            Event::Eof => break,
            _ => {}
        }
    }
    flush(&mut out, &mut buf, heading);
    Ok(out)
}

fn flush(out: &mut Vec<Block>, buf: &mut String, heading: Option<u8>) {
    // Yumuşak tire ve sıfır genişlikli boşluk şimdilik atılır; heceleme M3'te.
    let cleaned: String = buf
        .chars()
        .filter(|c| !matches!(c, '\u{00AD}' | '\u{200B}'))
        .collect();
    let text = collapse_ws(&cleaned);
    buf.clear();
    if text.is_empty() {
        return;
    }
    let kind = heading.map_or(BlockKind::Paragraph, BlockKind::Heading);
    out.push(Block { kind, text });
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
}
