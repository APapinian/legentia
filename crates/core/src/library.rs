//! Kütüphane: bir klasördeki EPUB ve PDF'leri (FB2, TXT, HTML EPUB sayılır) bulur, başlık ve yazarlarını önbellekle okur.
//! PDF'ler taranırken açılmaz (büyük belgede yavaş); başlık olarak dosya adı kullanılır.
//! Önbellek dosya yolu, boyutu ve değişiklik zamanıyla eşleşir; değişen kitap yeniden okunur.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::collate;
use crate::epub::Book;
use crate::storage::Store;

const INDEX_FILE: &str = "library.json";
/// Önbellekteki girdinin hangi tarama sürümüyle okunduğu; okunan alanlar
/// değişince artırılır, eski girdiler yeniden okunur.
const SCAN_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Format {
    #[default]
    Epub,
    Pdf,
}

impl Format {
    /// FB2, TXT ve HTML açılırken EPUB'a çevrildiği için EPUB sayılır.
    pub fn of(path: &Path) -> Option<Format> {
        let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        match ext.as_str() {
            "epub" | "fb2" | "txt" | "html" | "htm" | "xhtml" | "mobi" | "azw3" | "azw" => {
                Some(Format::Epub)
            }
            "zip" if name.ends_with(".fb2.zip") => Some(Format::Epub),
            // Çizgi romanlar PDF görüntüleyicisiyle açılır (`pdf/comic.rs`).
            "pdf" | "cbz" | "cbr" => Some(Format::Pdf),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub path: PathBuf,
    pub size: u64,
    pub modified: u64,
    /// Kayıt dosyalarının adı (`books/<id>.json`); bkz. [`book_id`].
    pub id: String,
    pub title: Option<String>,
    pub authors: Vec<String>,
    /// Açılamayan kitap da listede kalır; açmaya çalışınca hata gösterilir.
    pub readable: bool,
    #[serde(default)]
    pub format: Format,
    #[serde(default)]
    pub series: Option<String>,
    #[serde(default)]
    pub series_index: Option<f32>,
    #[serde(default)]
    pub scan: u32,
}

impl Entry {
    /// Gösterilecek başlık: kitabın başlığı, yoksa dosya adı.
    pub fn display_title(&self) -> String {
        self.title.clone().unwrap_or_else(|| {
            self.path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
    }
}

impl Entry {
    /// "Vakıf #2" ya da sırası bilinmiyorsa yalnız seri adı; seri yoksa `None`.
    pub fn series_label(&self) -> Option<String> {
        let name = self.series.as_deref()?;
        Some(match self.series_index {
            Some(i) if i.fract() == 0.0 => format!("{name} #{}", i as i64),
            Some(i) => format!("{name} #{}", i.to_string().replace('.', ",")),
            None => name.to_string(),
        })
    }
}

/// Kitabın kalıcı kimliği: EPUB'ın kendi tanımlayıcısı (ISBN/UUID), yoksa dosya adı ve boyutu.
/// Böylece kitap başka klasöre taşınsa da konumu ve yer imleri kaybolmaz.
pub fn book_id(identifier: Option<&str>, path: &Path, size: u64) -> String {
    let source = match identifier {
        Some(id) if !id.trim().is_empty() => format!("id:{}", id.trim()),
        _ => format!(
            "file:{}:{size}",
            path.file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        ),
    };
    format!("{:016x}", fnv1a(source.as_bytes()))
}

/// FNV-1a 64 bit: sürümden sürüme değişmeyen basit karma (std'nin karması kararlı değildir).
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// `root` altındaki EPUB ve PDF'leri tarar (gizli klasörler hariç) ve Türkçe başlık sırasıyla döner.
pub fn scan(root: &Path, store: &Store) -> Vec<Entry> {
    let cached: Vec<Entry> = store.load(INDEX_FILE).unwrap_or_default();
    let mut files = Vec::new();
    collect(root, &mut files);

    let mut entries = Vec::with_capacity(files.len());
    for path in files {
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        let size = meta.len();
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        if let Some(hit) = cached.iter().find(|e| {
            e.path == path && e.size == size && e.modified == modified && e.scan == SCAN_VERSION
        }) {
            entries.push(hit.clone());
            continue;
        }
        let format = Format::of(&path).unwrap_or_default();
        let opened = match format {
            Format::Epub => Book::open(&path).map(Some),
            Format::Pdf => Ok(None),
        };
        let entry = match opened {
            Ok(None) => Entry {
                id: book_id(None, &path, size),
                title: None,
                authors: Vec::new(),
                readable: true,
                format,
                path,
                size,
                modified,
                series: None,
                series_index: None,
                scan: SCAN_VERSION,
            },
            Ok(Some(book)) => Entry {
                id: book_id(book.identifier.as_deref(), &path, size),
                title: book.title,
                authors: book.authors,
                readable: true,
                format,
                path,
                size,
                modified,
                series: book.series,
                series_index: book.series_index,
                scan: SCAN_VERSION,
            },
            Err(e) => {
                log::warn!("cannot read {}: {e:?}", path.display());
                Entry {
                    id: book_id(None, &path, size),
                    title: None,
                    authors: Vec::new(),
                    readable: false,
                    format,
                    path,
                    size,
                    modified,
                    series: None,
                    series_index: None,
                    scan: SCAN_VERSION,
                }
            }
        };
        entries.push(entry);
    }
    entries.sort_by(|a, b| collate::compare(&a.display_title(), &b.display_title()));
    if entries != cached {
        store.save(INDEX_FILE, &entries);
    }
    entries
}

/// Kitapların taşınabileceği klasörler: kök ve gizli olmayan alt klasörler
/// (en çok iki düzey), Türkçe sırayla.
pub fn folders(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let hidden = path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'));
            if path.is_dir() && !hidden {
                out.push(path.clone());
                if depth < 2 {
                    walk(&path, depth + 1, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(root, 1, &mut out);
    out.sort_by(|a, b| crate::collate::compare(&a.to_string_lossy(), &b.to_string_lossy()));
    out.insert(0, root.to_path_buf());
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        if path.is_dir() {
            collect(&path, out);
        } else if Format::of(&path).is_some() {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epub::testutil::*;
    use crate::storage::temp_store;

    #[test]
    fn converted_formats_count_as_epub() {
        for name in ["a.fb2", "a.FB2.zip", "a.txt", "a.htm", "a.html", "a.epub"] {
            assert_eq!(Format::of(Path::new(name)), Some(Format::Epub), "{name}");
        }
        assert_eq!(Format::of(Path::new("a.zip")), None);
        assert_eq!(Format::of(Path::new("a.pdf")), Some(Format::Pdf));
    }

    #[test]
    fn id_is_stable_and_prefers_the_book_identifier() {
        let a = book_id(
            Some("urn:isbn:9789750719387"),
            Path::new("/a/kitap.epub"),
            10,
        );
        let b = book_id(
            Some("urn:isbn:9789750719387"),
            Path::new("/baska/yer.epub"),
            99,
        );
        assert_eq!(a, b, "taşınan kitap aynı kimliği korumalı");
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(
            book_id(None, Path::new("x.epub"), 1),
            book_id(None, Path::new("x.epub"), 2)
        );
    }

    #[test]
    fn scans_sorts_in_turkish_and_caches() {
        let store = temp_store("library");
        let root = store.dir().join("kitaplar");
        std::fs::create_dir_all(root.join("alt")).unwrap();
        std::fs::create_dir_all(root.join(".gizli")).unwrap();
        std::fs::write(root.join("cal.epub"), epub3_sample()).unwrap(); // Çalıkuşu
        std::fs::write(root.join("alt/kuyu.epub"), epub2_sample()).unwrap(); // Kuyucaklı Yusuf
        std::fs::write(root.join(".gizli/x.epub"), epub2_sample()).unwrap();
        std::fs::write(root.join("bozuk.epub"), b"zip degil").unwrap();
        std::fs::write(
            root.join("alt/Ödev.pdf"),
            crate::pdf::testutil::minimal_pdf("x"),
        )
        .unwrap();
        std::fs::write(root.join("resim.jpg"), b"kitap degil").unwrap();
        // Düz metin de kitaptır (açılırken EPUB'a çevrilir).
        std::fs::write(root.join("notlar.txt"), b"bir paragraf").unwrap();

        let entries = scan(&root, &store);
        let titles: Vec<_> = entries.iter().map(|e| e.display_title()).collect();
        assert_eq!(
            titles,
            vec!["bozuk", "Çalıkuşu", "Kuyucaklı Yusuf", "notlar", "Ödev"]
        );
        assert!(!entries[0].readable);
        assert!(entries[3].readable);
        assert_eq!(entries[4].format, Format::Pdf);
        assert!(store.dir().join("library.json").exists());

        // İkinci tarama önbellekten gelir ve aynı sonucu verir.
        assert_eq!(scan(&root, &store), entries);
    }
}
