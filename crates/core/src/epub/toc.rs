use std::io::{Read, Seek};

use quick_xml::events::Event;

use super::opf::Package;
use super::{EpubError, TocEntry, attr, collapse_ws, lenient_reader, read_entry, resolve_href};

/// İçindekileri EPUB3 nav belgesinden, yoksa EPUB2 NCX'ten okur.
/// Spine'da karşılığı olmayan girdiler atlanır.
pub(super) fn load<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    pkg: &Package,
) -> Result<Vec<TocEntry>, EpubError> {
    let nav = pkg
        .manifest
        .values()
        .find(|i| i.properties.split_whitespace().any(|p| p == "nav"));
    let ncx = pkg
        .ncx_id
        .as_ref()
        .and_then(|id| pkg.manifest.get(id))
        .or_else(|| {
            pkg.manifest
                .values()
                .find(|i| i.media_type == "application/x-dtbncx+xml")
        });

    let (path, raw) = if let Some(item) = nav {
        let path = resolve_href(&pkg.opf_path, &item.href)?;
        let text = read_entry(archive, &path)?;
        let raw = parse_nav(&text, &path)?;
        (path, raw)
    } else if let Some(item) = ncx {
        let path = resolve_href(&pkg.opf_path, &item.href)?;
        let text = read_entry(archive, &path)?;
        let raw = parse_ncx(&text, &path)?;
        (path, raw)
    } else {
        return Ok(Vec::new());
    };

    let mut toc = Vec::new();
    for (title, depth, href) in raw {
        let Ok(target) = resolve_href(&path, &href) else {
            continue;
        };
        if let Some(spine_index) = pkg.spine.iter().position(|s| *s == target)
            && !title.is_empty()
        {
            toc.push(TocEntry {
                title,
                depth,
                spine_index,
            });
        }
    }
    Ok(toc)
}

type RawEntry = (String, u32, String);

fn xml_err(entry: &str, e: impl std::fmt::Display) -> EpubError {
    EpubError::Xml {
        entry: entry.to_string(),
        detail: e.to_string(),
    }
}

/// `<nav epub:type="toc">` içindeki `<a>` bağlantıları; derinlik iç içe `<ol>` sayısıdır.
fn parse_nav(text: &str, path: &str) -> Result<Vec<RawEntry>, EpubError> {
    let mut reader = lenient_reader(text);
    let mut out = Vec::new();
    // Toc nav'ının içindeyken açık <nav> sayısı; dışarıdayken 0.
    let mut in_toc = 0u32;
    let mut first_nav_seen = false;
    let mut found_toc = false;
    let mut ol_depth = 0u32;
    let mut link: Option<(String, String)> = None;

    loop {
        match reader.read_event().map_err(|e| xml_err(path, e))? {
            Event::Start(e) => match e.local_name().as_ref() {
                "nav" => {
                    let is_toc =
                        attr(&e, "type").is_some_and(|t| t.split_whitespace().any(|v| v == "toc"));
                    if in_toc > 0 {
                        in_toc += 1;
                    } else if is_toc || (!first_nav_seen && !found_toc) {
                        in_toc = 1;
                        found_toc |= is_toc;
                        out.clear();
                    }
                    first_nav_seen = true;
                }
                "ol" if in_toc > 0 => ol_depth += 1,
                "a" if in_toc > 0 => link = attr(&e, "href").map(|h| (h, String::new())),
                _ => {}
            },
            Event::Text(t) => {
                if let Some((_, label)) = link.as_mut() {
                    label.push_str(&t.xml10_content());
                }
            }
            Event::GeneralRef(r) => {
                if let Some((_, label)) = link.as_mut() {
                    super::xhtml::push_ref(label, &r);
                }
            }
            Event::End(e) => match e.local_name().as_ref() {
                "nav" if in_toc > 0 => {
                    in_toc -= 1;
                    if in_toc == 0 && found_toc {
                        break;
                    }
                }
                "ol" if in_toc > 0 => ol_depth = ol_depth.saturating_sub(1),
                "a" => {
                    if let Some((href, label)) = link.take() {
                        out.push((collapse_ws(&label), ol_depth.saturating_sub(1), href));
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

/// NCX `navPoint` ağacı: etiket `navLabel/text`, hedef `content@src`.
fn parse_ncx(text: &str, path: &str) -> Result<Vec<RawEntry>, EpubError> {
    let mut reader = lenient_reader(text);
    let mut out = Vec::new();
    let mut depth = 0u32;
    let mut label = String::new();
    let mut in_text = false;

    loop {
        match reader.read_event().map_err(|e| xml_err(path, e))? {
            Event::Start(e) => match e.local_name().as_ref() {
                "navPoint" => {
                    depth += 1;
                    label.clear();
                }
                "text" => in_text = true,
                "content" => push_ncx(&mut out, &e, &label, depth),
                _ => {}
            },
            Event::Empty(e) if e.local_name().as_ref() == "content" => {
                push_ncx(&mut out, &e, &label, depth)
            }
            Event::Text(t) if in_text => label.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if in_text => super::xhtml::push_ref(&mut label, &r),
            Event::End(e) => match e.local_name().as_ref() {
                "navPoint" => depth = depth.saturating_sub(1),
                "text" => in_text = false,
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

fn push_ncx(out: &mut Vec<RawEntry>, e: &quick_xml::events::BytesStart, label: &str, depth: u32) {
    if depth > 0
        && let Some(src) = attr(e, "src")
    {
        out.push((collapse_ws(label), depth - 1, src));
    }
}
