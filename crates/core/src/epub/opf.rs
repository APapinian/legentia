use std::collections::HashMap;

use quick_xml::Reader;
use quick_xml::events::Event;

use super::{EpubError, attr, collapse_ws, resolve_href};

pub(super) struct ManifestItem {
    pub href: String,
    pub media_type: String,
    pub properties: String,
}

pub(super) struct Package {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
    /// İlk `dc:identifier` (ISBN, UUID…); kitabın kimliği için.
    pub identifier: Option<String>,
    pub spine: Vec<String>,
    /// Kapak görseli (paket köküne göre yol): EPUB3'te `cover-image` özellikli
    /// öğe, EPUB2'de `<meta name="cover">`ın gösterdiği öğe.
    pub cover: Option<String>,
    /// Seri adı ve kitabın serideki sırası: calibre'nin `calibre:series`
    /// meta'sı ya da EPUB3 `belongs-to-collection` + `group-position`.
    pub series: Option<String>,
    pub series_index: Option<f32>,
    pub manifest: HashMap<String, ManifestItem>,
    /// EPUB2: spine'ın `toc` özniteliğindeki NCX öğesinin kimliği.
    pub ncx_id: Option<String>,
    pub opf_path: String,
}

fn xml_err(entry: &str, e: impl std::fmt::Display) -> EpubError {
    EpubError::Xml {
        entry: entry.to_string(),
        detail: e.to_string(),
    }
}

/// `META-INF/container.xml` içindeki ilk OPF dosyasının yolu.
pub(super) fn root_file(container: &str) -> Result<String, EpubError> {
    let mut reader = Reader::from_str(container);
    loop {
        match reader
            .read_event()
            .map_err(|e| xml_err("META-INF/container.xml", e))?
        {
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == "rootfile" => {
                if let Some(path) = attr(&e, "full-path") {
                    return Ok(path);
                }
            }
            Event::Eof => return Err(EpubError::NoRootFile),
            _ => {}
        }
    }
}

pub(super) fn parse(text: &str, opf_path: &str) -> Result<Package, EpubError> {
    let mut reader = Reader::from_str(text);
    let mut pkg = Package {
        title: None,
        authors: Vec::new(),
        language: None,
        identifier: None,
        spine: Vec::new(),
        cover: None,
        series: None,
        series_index: None,
        manifest: HashMap::new(),
        ncx_id: None,
        opf_path: opf_path.to_string(),
    };
    let mut spine_ids = Vec::new();
    let mut cover_id = None;
    let mut series = Series::default();
    // Şu an metni toplanan meta veri alanı ("title", "creator", "language";
    // EPUB3 seri meta'larında "collection:<kimlik>", "position:<kimlik>").
    let mut field: Option<String> = None;
    let mut buf = String::new();

    loop {
        match reader.read_event().map_err(|e| xml_err(opf_path, e))? {
            Event::Start(e) => {
                let name = e.local_name().as_ref().to_string();
                match name.as_str() {
                    "title" | "creator" | "language" | "identifier" => {
                        field = Some(name);
                        buf.clear();
                    }
                    "spine" => pkg.ncx_id = attr(&e, "toc"),
                    "item" => add_item(&mut pkg, &e),
                    "meta" => {
                        cover_meta(&e, &mut cover_id);
                        let id = attr(&e, "id").unwrap_or_default();
                        let refines = attr(&e, "refines").unwrap_or_default();
                        field = match attr(&e, "property").as_deref() {
                            Some("belongs-to-collection") => Some(format!("collection:{id}")),
                            Some("group-position") => {
                                Some(format!("position:{}", refines.trim_start_matches('#')))
                            }
                            _ => None,
                        };
                        buf.clear();
                    }
                    _ => {}
                }
            }
            Event::Empty(e) => match e.local_name().as_ref() {
                "item" => add_item(&mut pkg, &e),
                "meta" => {
                    cover_meta(&e, &mut cover_id);
                    match attr(&e, "name").as_deref() {
                        Some("calibre:series") => series.calibre = attr(&e, "content"),
                        Some("calibre:series_index") => series.calibre_index = attr(&e, "content"),
                        _ => {}
                    }
                }
                "itemref" => {
                    if attr(&e, "linear").as_deref() != Some("no")
                        && let Some(id) = attr(&e, "idref")
                    {
                        spine_ids.push(id);
                    }
                }
                _ => {}
            },
            Event::Text(t) if field.is_some() => buf.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if field.is_some() => super::xhtml::push_ref(&mut buf, &r),
            Event::End(e) => {
                let local = e.local_name();
                let closes = match field.as_deref() {
                    Some(f) if f.starts_with("collection:") || f.starts_with("position:") => {
                        local.as_ref() == "meta"
                    }
                    Some(f) => f == local.as_ref(),
                    None => false,
                };
                if closes {
                    let value = collapse_ws(&buf);
                    if !value.is_empty() {
                        match field.as_deref() {
                            Some(f) if f.starts_with("collection:") => {
                                series
                                    .collections
                                    .push((f["collection:".len()..].into(), value));
                            }
                            Some(f) if f.starts_with("position:") => {
                                series
                                    .positions
                                    .push((f["position:".len()..].into(), value));
                            }
                            Some("title") if pkg.title.is_none() => pkg.title = Some(value),
                            Some("creator") => pkg.authors.push(value),
                            Some("language") if pkg.language.is_none() => {
                                pkg.language = Some(value)
                            }
                            Some("identifier") if pkg.identifier.is_none() => {
                                pkg.identifier = Some(value)
                            }
                            _ => {}
                        }
                    }
                    field = None;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    for id in spine_ids {
        match pkg.manifest.get(&id) {
            Some(item) => pkg.spine.push(resolve_href(opf_path, &item.href)?),
            None => log::warn!("spine refers to unknown manifest id {id}"),
        }
    }
    let cover = pkg
        .manifest
        .values()
        .find(|i| i.properties.split_whitespace().any(|p| p == "cover-image"))
        .or_else(|| pkg.manifest.get(cover_id.as_ref()?));
    if let Some(item) = cover {
        pkg.cover = resolve_href(opf_path, &item.href).ok();
    }
    (pkg.series, pkg.series_index) = series.resolve();
    Ok(pkg)
}

/// Seri meta'ları; ayrıştırma bitince [`Series::resolve`].
#[derive(Default)]
struct Series {
    calibre: Option<String>,
    calibre_index: Option<String>,
    /// EPUB3: (meta kimliği, seri adı) ve (gösterdiği meta kimliği, sıra).
    collections: Vec<(String, String)>,
    positions: Vec<(String, String)>,
}

impl Series {
    /// calibre bilgisi önce; yoksa ilk koleksiyon ve onun sırası.
    fn resolve(self) -> (Option<String>, Option<f32>) {
        let number = |s: &str| s.trim().replace(',', ".").parse::<f32>().ok();
        if let Some(name) = self.calibre.filter(|n| !n.trim().is_empty()) {
            return (
                Some(name.trim().to_string()),
                self.calibre_index.as_deref().and_then(number),
            );
        }
        let Some((id, name)) = self.collections.into_iter().next() else {
            return (None, None);
        };
        let index = self
            .positions
            .iter()
            .find(|(r, _)| !id.is_empty() && *r == id)
            .and_then(|(_, p)| number(p));
        (Some(name), index)
    }
}

/// EPUB2: `<meta name="cover" content="<öğe kimliği>"/>`.
fn cover_meta(e: &quick_xml::events::BytesStart, cover_id: &mut Option<String>) {
    if attr(e, "name").as_deref() == Some("cover") {
        *cover_id = attr(e, "content");
    }
}

fn add_item(pkg: &mut Package, e: &quick_xml::events::BytesStart) {
    if let (Some(id), Some(href)) = (attr(e, "id"), attr(e, "href")) {
        pkg.manifest.insert(
            id,
            ManifestItem {
                href,
                media_type: attr(e, "media-type").unwrap_or_default(),
                properties: attr(e, "properties").unwrap_or_default(),
            },
        );
    }
}
