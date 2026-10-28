//! EPUB okuma: ZIP paketi, OPF (meta veri, manifest, spine), içindekiler ve
//! XHTML'den düz metin blokları.
//!
//! Güvenlik: dosyalar diske çıkarılmaz, yalnızca bellekte okunur. Her girdi
//! okunurken boyut sınırı uygulanır (bildirilen boyuta güvenilmez), kitap
//! içindeki yollar paket kökünün dışına çıkamaz.

mod opf;
mod toc;
mod xhtml;

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use crate::i18n::Msg;
pub(crate) use xhtml::push_ref;
pub use xhtml::{Block, BlockKind};

/// Tek bir XML/XHTML girdisi için üst sınır (açılmış boyut).
const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024;
/// Paketteki girdi sayısı için üst sınır.
const MAX_ENTRIES: usize = 20_000;

#[derive(Debug)]
pub enum EpubError {
    Io(std::io::Error),
    NotZip(String),
    MissingEntry(String),
    TooLarge(String),
    TooManyEntries(usize),
    UnsafePath(String),
    Xml { entry: String, detail: String },
    NoRootFile,
    EmptySpine,
}

impl EpubError {
    /// Kullanıcıya gösterilecek Türkçe açıklama. Teknik ayrıntı `Debug` ile log'a gider.
    pub fn message(&self) -> Msg {
        match self {
            EpubError::Io(_) => Msg::BookFileUnreadable,
            EpubError::NotZip(_) => Msg::BookNotEpub,
            EpubError::TooLarge(_) | EpubError::TooManyEntries(_) => Msg::BookTooLarge,
            EpubError::UnsafePath(_) => Msg::BookUnsafe,
            EpubError::MissingEntry(_)
            | EpubError::Xml { .. }
            | EpubError::NoRootFile
            | EpubError::EmptySpine => Msg::BookDamaged,
        }
    }
}

impl From<std::io::Error> for EpubError {
    fn from(e: std::io::Error) -> Self {
        EpubError::Io(e)
    }
}

/// İçindekiler girdisi. `spine_index` bölümün okuma sırasındaki yeridir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    pub title: String,
    pub depth: u32,
    pub spine_index: usize,
}

pub struct Book {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub identifier: Option<String>,
    /// Okuma sırasındaki XHTML dosyaları (paket köküne göre yol).
    pub spine: Vec<String>,
    /// Bölüm dosyalarının açılmış boyutu (bayt; ZIP başlığından, okunmadan).
    /// Kalan okuma süresi tahmininde metin uzunluğunun yaklaşığı.
    pub sizes: Vec<u64>,
    pub toc: Vec<TocEntry>,
    archive: zip::ZipArchive<Box<dyn ReadSeek>>,
}

trait ReadSeek: Read + std::io::Seek {}
impl<T: Read + std::io::Seek> ReadSeek for T {}

impl Book {
    pub fn open(path: &Path) -> Result<Book, EpubError> {
        let file = BufReader::new(File::open(path)?);
        Self::from_reader(Box::new(file))
    }

    fn from_reader(reader: Box<dyn ReadSeek>) -> Result<Book, EpubError> {
        let mut archive =
            zip::ZipArchive::new(reader).map_err(|e| EpubError::NotZip(e.to_string()))?;
        if archive.len() > MAX_ENTRIES {
            return Err(EpubError::TooManyEntries(archive.len()));
        }

        let container = read_entry(&mut archive, "META-INF/container.xml")?;
        let opf_path = opf::root_file(&container)?;
        let opf_text = read_entry(&mut archive, &opf_path)?;
        let package = opf::parse(&opf_text, &opf_path)?;
        if package.spine.is_empty() {
            return Err(EpubError::EmptySpine);
        }

        let toc = match toc::load(&mut archive, &package) {
            Ok(toc) if !toc.is_empty() => toc,
            Ok(_) => Vec::new(),
            Err(e) => {
                log::warn!("table of contents unreadable, falling back to spine: {e:?}");
                Vec::new()
            }
        };

        let sizes = package
            .spine
            .iter()
            .map(|p| archive.by_name(p).map_or(0, |e| e.size()))
            .collect();
        Ok(Book {
            sizes,
            title: package.title,
            authors: package.authors,
            language: package.language,
            identifier: package.identifier,
            spine: package.spine,
            toc,
            archive,
        })
    }

    /// Bir bölümün metin bloklarını okur. Bölümler istendiğinde okunur;
    /// kitabın tamamı belleğe alınmaz.
    pub fn chapter(&mut self, spine_index: usize) -> Result<Vec<Block>, EpubError> {
        let path = self
            .spine
            .get(spine_index)
            .ok_or_else(|| EpubError::MissingEntry(format!("spine #{spine_index}")))?
            .clone();
        let text = read_entry(&mut self.archive, &path)?;
        xhtml::blocks(&text, &path)
    }
}

/// Bir girdiyi boyut sınırıyla UTF-8 metin olarak okur.
fn read_entry<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<String, EpubError> {
    let entry = archive
        .by_name(name)
        .map_err(|_| EpubError::MissingEntry(name.to_string()))?;
    let mut bytes = Vec::new();
    entry.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_ENTRY_BYTES {
        return Err(EpubError::TooLarge(name.to_string()));
    }
    // Bazı kitaplar UTF-8 BOM ile başlar.
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    String::from_utf8(bytes.to_vec()).map_err(|e| EpubError::Xml {
        entry: name.to_string(),
        detail: format!("not UTF-8: {e}"),
    })
}

/// `href`'i `base` dosyasının klasörüne göre çözer; parça (#...) ve sorgu atılır,
/// %XX kodları açılır. Paket kökünün dışına çıkan yollar reddedilir.
pub(crate) fn resolve_href(base: &str, href: &str) -> Result<String, EpubError> {
    let href = href.split(['#', '?']).next().unwrap_or("");
    let href = percent_decode(href);
    if href.contains("://") || href.starts_with('/') {
        return Err(EpubError::UnsafePath(href));
    }
    let mut parts: Vec<&str> = base.split('/').collect();
    parts.pop(); // dosya adı
    for seg in href.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(EpubError::UnsafePath(href.clone()));
                }
            }
            s => parts.push(s),
        }
    }
    Ok(parts.join("/"))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// Öznitelik değerini yerel adına göre bulur (ad alanı öneki yok sayılır).
pub(crate) fn attr(e: &quick_xml::events::BytesStart, name: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == name)
        .map(|a| {
            a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map(|v| v.into_owned())
                .unwrap_or_else(|_| a.value.into_owned())
        })
}

/// Uyuşmayan ya da fazladan kapanış etiketlerine hoşgörülü XML okuyucu.
pub(crate) fn lenient_reader(text: &str) -> quick_xml::Reader<&[u8]> {
    let mut reader = quick_xml::Reader::from_str(text);
    reader.config_mut().check_end_names = false;
    reader.config_mut().allow_unmatched_ends = true;
    reader
}

/// Metindeki boşlukları tek boşluğa indirir ve NFC'ye normalleştirir.
/// Bazı kitaplar "İ"yi `I` + U+0307, "Ş"yi `S` + U+0327 olarak yazar;
/// NFC bunları tek karaktere birleştirir.
pub(crate) fn collapse_ws(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .nfc()
        .collect()
}

#[cfg(test)]
pub(crate) mod testutil;

#[cfg(test)]
mod tests {
    use super::*;
    use testutil::*;

    #[test]
    fn resolves_relative_paths() {
        assert_eq!(
            resolve_href("OEBPS/content.opf", "text/ch1.xhtml").unwrap(),
            "OEBPS/text/ch1.xhtml"
        );
        assert_eq!(
            resolve_href("OEBPS/text/ch1.xhtml", "../img/a.png").unwrap(),
            "OEBPS/img/a.png"
        );
        assert_eq!(
            resolve_href("OEBPS/content.opf", "ch%201.xhtml#p3").unwrap(),
            "OEBPS/ch 1.xhtml"
        );
        assert_eq!(
            resolve_href("content.opf", "Bölüm.xhtml").unwrap(),
            "Bölüm.xhtml"
        );
    }

    #[test]
    fn rejects_paths_escaping_the_package() {
        assert!(matches!(
            resolve_href("OEBPS/content.opf", "../../etc/passwd"),
            Err(EpubError::UnsafePath(_))
        ));
        assert!(matches!(
            resolve_href("content.opf", "/etc/passwd"),
            Err(EpubError::UnsafePath(_))
        ));
        assert!(matches!(
            resolve_href("content.opf", "http://x.y/a"),
            Err(EpubError::UnsafePath(_))
        ));
    }

    #[test]
    fn opens_epub3_with_turkish_metadata_and_nav() {
        let mut book = open_bytes(epub3_sample()).unwrap();
        assert_eq!(book.title.as_deref(), Some("Çalıkuşu"));
        assert_eq!(book.authors, vec!["Reşat Nuri Güntekin"]);
        assert_eq!(book.language.as_deref(), Some("tr"));
        assert_eq!(book.identifier.as_deref(), Some("urn:uuid:calikusu-0001"));
        assert_eq!(book.spine.len(), 2);
        let titles: Vec<_> = book
            .toc
            .iter()
            .map(|t| (t.title.as_str(), t.depth, t.spine_index))
            .collect();
        assert_eq!(
            titles,
            vec![
                ("Birinci Kısım", 0, 0),
                ("İkinci Kısım", 0, 1),
                ("Ağaçlar", 1, 1)
            ]
        );
        let blocks = book.chapter(0).unwrap();
        assert_eq!(blocks[0].kind, BlockKind::Heading(1));
        assert_eq!(blocks[0].text, "Birinci Kısım");
        assert_eq!(
            blocks[1].text,
            "İğne ile kuyu kazmak; şöyle ölçülü, çok güzel."
        );
    }

    #[test]
    fn opens_epub2_with_ncx() {
        let book = open_bytes(epub2_sample()).unwrap();
        assert_eq!(book.title.as_deref(), Some("Kuyucaklı Yusuf"));
        let titles: Vec<_> = book
            .toc
            .iter()
            .map(|t| (t.title.as_str(), t.spine_index))
            .collect();
        assert_eq!(titles, vec![("Bir", 0), ("İki", 1)]);
    }

    #[test]
    fn not_a_zip_is_reported_as_not_epub() {
        let err = open_bytes(b"bu bir zip degil".to_vec()).err().unwrap();
        assert_eq!(err.message(), Msg::BookNotEpub);
    }

    #[test]
    fn truncated_zip_does_not_panic() {
        let mut bytes = epub3_sample();
        bytes.truncate(bytes.len() / 2);
        assert!(open_bytes(bytes).is_err());
    }

    #[test]
    fn missing_container_is_damaged() {
        let bytes = zip_of(&[("mimetype", "application/epub+zip")]);
        assert_eq!(open_bytes(bytes).err().unwrap().message(), Msg::BookDamaged);
    }

    #[test]
    fn missing_opf_is_damaged() {
        let bytes = zip_of(&[("META-INF/container.xml", &container("OEBPS/yok.opf"))]);
        assert_eq!(open_bytes(bytes).err().unwrap().message(), Msg::BookDamaged);
    }

    #[test]
    fn broken_chapter_xml_is_an_error_not_a_panic() {
        let mut book = open_bytes(epub_with_chapter(
            "<html><body><p>açık kalan <b>etiket</p></body>",
        ))
        .unwrap();
        // Hatalı etiket kapanışında bile ya okunur metin ya da hata döner.
        match book.chapter(0) {
            Ok(blocks) => assert!(blocks.iter().any(|b| b.text.contains("açık kalan"))),
            Err(e) => assert_eq!(e.message(), Msg::BookDamaged),
        }
    }

    #[test]
    fn oversized_entry_is_rejected() {
        let huge = "a".repeat((MAX_ENTRY_BYTES + 10) as usize);
        let mut book = open_bytes(epub_with_chapter(&huge)).unwrap();
        assert_eq!(book.chapter(0).err().unwrap().message(), Msg::BookTooLarge);
    }

    #[test]
    fn manifest_item_outside_package_is_rejected() {
        let opf = opf_doc(
            r#"<item id="c" href="../../dis.xhtml" media-type="application/xhtml+xml"/>"#,
            r#"<itemref idref="c"/>"#,
        );
        let bytes = zip_of(&[
            ("META-INF/container.xml", &container("OEBPS/content.opf")),
            ("OEBPS/content.opf", &opf),
        ]);
        assert_eq!(open_bytes(bytes).err().unwrap().message(), Msg::BookUnsafe);
    }
}
