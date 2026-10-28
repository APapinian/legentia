//! Sözlükler: çekimli Türkçe kelimeden kök bulma ve sözlük sağlayıcıları.
//!
//! Her biçim [`Dictionary`]'yi gerçekler. Anahtarlar [`key`] ile Türkçe küçük
//! harfe ve şapkasız biçime indirilir; arama büyük/küçük harf ve şapkadan
//! bağımsızdır ("kagıt" → "kâğıt"). Sözlük dosyaları kullanıcınındır ve
//! güvenilmez girdi sayılır: boyutlar sınırlıdır, bozuk dosya hata verir.

use std::path::{Path, PathBuf};

pub(crate) mod dictzip;
mod index;
pub mod stardict;
pub mod stem;

pub(crate) use index::Index;

/// Tek dosyanın belleğe alınabilecek en büyük boyutu (dizin dosyaları).
pub(crate) const MAX_INDEX_BYTES: u64 = 256 * 1024 * 1024;
/// Tek maddenin en büyük boyutu.
pub(crate) const MAX_ARTICLE_BYTES: u32 = 4 * 1024 * 1024;

#[derive(Debug)]
pub enum DictError {
    Io(std::io::Error),
    Invalid(String),
}

impl From<std::io::Error> for DictError {
    fn from(e: std::io::Error) -> Self {
        DictError::Io(e)
    }
}

/// Madde metninin biçimi; ekranda düz metne ve basit vurgulara çevrilir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Markup {
    Plain,
    Html,
    /// GTK Pango biçimlendirmesi (StarDict "g").
    Pango,
    Xdxf,
    /// ABBYY Lingvo DSL biçimlendirmesi.
    Dsl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Article {
    /// Sözlükte yazıldığı biçimiyle madde başı.
    pub headword: String,
    pub parts: Vec<(Markup, String)>,
}

pub trait Dictionary {
    fn name(&self) -> &str;
    /// `key` ([`key`] ile indirgenmiş) sözlükte var mı?
    fn contains(&self, key: &str) -> bool;
    /// `key`'in maddeleri (eş sesliler ayrı madde).
    fn lookup(&self, key: &str) -> Result<Vec<Article>, DictError>;
}

/// Arama anahtarı: Türkçe küçük harf, NFC, â/î/û şapkasız.
pub fn key(word: &str) -> String {
    stem::normalize(word.trim())
        .chars()
        .map(|c| match c {
            'â' => 'a',
            'î' => 'i',
            'û' => 'u',
            _ => c,
        })
        .collect()
}

/// Bir kelimenin sözlük sonucu: bulunan biçim (kök) ve her sözlükten maddeler.
#[derive(Debug)]
pub struct Definition {
    pub form: String,
    pub articles: Vec<(String, Article)>,
}

/// Açılmış sözlükler, bulunma sırasıyla.
#[derive(Default)]
pub struct Dictionaries {
    dicts: Vec<Box<dyn Dictionary>>,
}

impl Dictionaries {
    /// `dirs` altındaki (alt klasörler dahil) sözlükleri açar. Açılamayanlar
    /// yolu ve hatasıyla döner; diğerleri kullanılır.
    pub fn open(dirs: &[PathBuf]) -> (Self, Vec<(PathBuf, DictError)>) {
        let mut files = Vec::new();
        for dir in dirs {
            collect(dir, 0, &mut files);
        }
        files.sort();
        let mut out = Self::default();
        let mut failed = Vec::new();
        for path in files {
            match stardict::StarDict::open(&path) {
                Ok(d) => out.dicts.push(Box::new(d)),
                Err(e) => failed.push((path, e)),
            }
        }
        (out, failed)
    }

    pub fn is_empty(&self) -> bool {
        self.dicts.is_empty()
    }

    pub fn names(&self) -> Vec<&str> {
        self.dicts.iter().map(|d| d.name()).collect()
    }

    /// Kelimeyi ya da sözlükte bulunan ilk kökünü bütün sözlüklerde arar.
    pub fn define(&self, word: &str) -> Option<Definition> {
        let form = stem::lookup_form(word, |c| {
            let k = key(c);
            self.dicts.iter().any(|d| d.contains(&k))
        })?;
        let k = key(&form);
        let articles = self
            .dicts
            .iter()
            .flat_map(|d| match d.lookup(&k) {
                Ok(a) => a.into_iter().map(|a| (d.name().to_string(), a)).collect(),
                Err(e) => {
                    log::warn!("lookup in {} failed: {e:?}", d.name());
                    Vec::new()
                }
            })
            .collect();
        Some(Definition { form, articles })
    }
}

/// Sözlük dosyalarını (StarDict .ifo) toplar; gizli klasörler dahil, derinlik sınırlı.
fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            collect(&path, depth + 1, out);
        } else if path.extension().is_some_and(|x| x == "ifo") {
            out.push(path);
        }
    }
}

#[cfg(test)]
pub(crate) mod testutil;

#[cfg(test)]
mod tests {
    use super::testutil::{stardict, temp_dir};
    use super::*;

    #[test]
    fn inflected_words_are_defined_from_every_dictionary() {
        let dir = temp_dir("dicts");
        let sub = dir.join("ikinci");
        std::fs::create_dir_all(&sub).unwrap();
        stardict(
            &dir,
            "bir",
            &[("kitap", "basılı eser"), ("gelmek", "varmak")],
            &[],
            true,
        );
        stardict(&sub, "iki", &[("Kitap", "book")], &[], false);
        std::fs::write(dir.join("bozuk.ifo"), "değil").unwrap();
        let (dicts, failed) = Dictionaries::open(std::slice::from_ref(&dir));
        assert_eq!(dicts.names(), vec!["bir sözlüğü", "iki sözlüğü"]);
        assert_eq!(failed.len(), 1, "bozuk sözlük ötekileri engellemez");

        let d = dicts.define("Kitaplarımızdan").unwrap();
        assert_eq!(d.form, "kitap");
        let texts: Vec<_> = d
            .articles
            .iter()
            .map(|(n, a)| (n.as_str(), a.parts[0].1.as_str()))
            .collect();
        assert_eq!(
            texts,
            vec![("bir sözlüğü", "basılı eser"), ("iki sözlüğü", "book")]
        );
        assert_eq!(dicts.define("geliyorum").unwrap().form, "gelmek");
        assert!(dicts.define("qwxz").is_none());
    }
}
