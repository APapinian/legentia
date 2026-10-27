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
        manifest: HashMap::new(),
        ncx_id: None,
        opf_path: opf_path.to_string(),
    };
    let mut spine_ids = Vec::new();
    // Şu an metni toplanan meta veri alanı ("title", "creator", "language").
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
                    _ => {}
                }
            }
            Event::Empty(e) => match e.local_name().as_ref() {
                "item" => add_item(&mut pkg, &e),
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
                if field.as_deref() == Some(e.local_name().as_ref()) {
                    let value = collapse_ws(&buf);
                    if !value.is_empty() {
                        match field.as_deref() {
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
    Ok(pkg)
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
