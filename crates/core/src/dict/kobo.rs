//! Kobo dicthtml sözlükleri (`dicthtml-*.zip`, Nickel'ın biçimi).
//!
//! Zip arşivinin kökünde `PREFIX.html` dosyaları (çoğunlukla gzip'li) ve
//! `words` (marisa-trie) bulunur. Her madde `<w>…</w>` içindedir; madde başı
//! `<a name="…" />`, çekimli biçimler `<variant name="…"/>` ile verilir.
//! `words` dosyası ve Kobo'nun 2 harflik önek kuralı kullanılmaz: dizin
//! açılışta HTML dosyaları bir kez taranarak kurulur.
//!
//! Kobo'nun resmî sözlükleri AES ile şifrelidir; anahtarı belgelenmemiştir ve
//! açmak kopya korumasını aşmak olur. Şifreli dosya hata verir, desteklenmez.
//! Kaynak: pgaskin.net/dictutil/dicthtml.

use std::cell::RefCell;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use super::render::entities;
use super::{Article, DictError, Dictionary, Index, MAX_INDEX_BYTES, Markup, key};

fn invalid(what: impl Into<String>) -> DictError {
    DictError::Invalid(what.into())
}

pub struct Kobo {
    name: String,
    archive: RefCell<zip::ZipArchive<BufReader<File>>>,
    /// HTML dosyalarının arşivdeki sıraları.
    files: Vec<usize>,
    /// Maddeler: (dosya, açılmış dosyada `<w>` aralığı, madde başı).
    entries: Vec<(u32, u32, u32, String)>,
    index: Index,
}

/// Zip içindeki dosyayı açar: gzip ise çözülür, düz HTML olduğu gibi; ikisi
/// de değilse şifrelidir.
fn read_html(
    archive: &mut zip::ZipArchive<BufReader<File>>,
    i: usize,
) -> Result<String, DictError> {
    let mut raw = Vec::new();
    archive
        .by_index(i)
        .map_err(|e| invalid(e.to_string()))?
        .take(MAX_INDEX_BYTES)
        .read_to_end(&mut raw)?;
    let bytes = if raw.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&raw[..])
            .take(MAX_INDEX_BYTES)
            .read_to_end(&mut out)?;
        out
    } else if raw.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'<') {
        raw
    } else {
        return Err(invalid("encrypted dicthtml (Kobo DRM) is not supported"));
    };
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// `tag` içinde `attr="…"` değeri.
fn attr(tag: &str, attr: &str) -> Option<String> {
    let at = tag.find(&format!("{attr}=\""))? + attr.len() + 2;
    let end = tag[at..].find('"')? + at;
    Some(entities(&tag[at..end]))
}

/// HTML'deki maddeler: (`<w>` başı, sonu, madde başı, anahtarlar).
fn entries(html: &str) -> Vec<(usize, usize, String, Vec<String>)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(s) = html[at..].find("<w>").map(|i| at + i) {
        let Some(e) = html[s..].find("</w>").map(|i| s + i + 4) else {
            break;
        };
        let w = &html[s..e];
        let mut keys = Vec::new();
        let mut headword = String::new();
        let mut p = 0;
        while let Some(t) = w[p..].find('<').map(|i| p + i) {
            let end = w[t..].find('>').map_or(w.len(), |i| t + i + 1);
            let tag = &w[t..end];
            if (tag.starts_with("<a ") || tag.starts_with("<variant "))
                && let Some(name) = attr(tag, "name")
            {
                if tag.starts_with("<a ") && headword.is_empty() {
                    headword = name.trim().to_string();
                }
                keys.push(key(&name));
            }
            p = end;
        }
        keys.retain(|k| !k.is_empty());
        if !keys.is_empty() {
            out.push((s, e, headword, keys));
        }
        at = e;
    }
    out
}

impl Kobo {
    pub fn open(path: &Path) -> Result<Kobo, DictError> {
        let file = BufReader::new(File::open(path)?);
        let mut archive = zip::ZipArchive::new(file).map_err(|e| invalid(e.to_string()))?;
        let mut files = Vec::new();
        let mut list = Vec::new();
        let mut pairs = Vec::new();
        for i in 0..archive.len() {
            let name = archive
                .by_index_raw(i)
                .map_err(|e| invalid(e.to_string()))?
                .name()
                .map_err(|e| invalid(e.to_string()))?
                .to_string();
            if !name.ends_with(".html") || name.contains('/') {
                continue;
            }
            let html = read_html(&mut archive, i)?;
            let f = files.len() as u32;
            files.push(i);
            for (s, e, headword, keys) in entries(&html) {
                let id = list.len() as u32;
                list.push((f, s as u32, e as u32, headword));
                pairs.extend(keys.into_iter().map(|k| (k, id)));
            }
        }
        if list.is_empty() {
            return Err(invalid("no entries in dicthtml"));
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Kobo {
            name,
            archive: RefCell::new(archive),
            files,
            entries: list,
            index: Index::build(pairs),
        })
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

impl Dictionary for Kobo {
    fn name(&self) -> &str {
        &self.name
    }

    fn contains(&self, key: &str) -> bool {
        self.index.contains(key)
    }

    fn lookup(&self, key: &str) -> Result<Vec<Article>, DictError> {
        let mut out = Vec::new();
        let mut cached: Option<(u32, String)> = None;
        for id in self.index.find(key) {
            let (f, s, e, headword) = &self.entries[id as usize];
            if cached.as_ref().is_none_or(|(cf, _)| cf != f) {
                let html = read_html(&mut self.archive.borrow_mut(), self.files[*f as usize])?;
                cached = Some((*f, html));
            }
            let html = &cached.as_ref().expect("az önce yüklendi").1;
            let body = html
                .get(*s as usize..*e as usize)
                .ok_or_else(|| invalid("entry range"))?;
            out.push(Article {
                headword: headword.clone(),
                parts: vec![(Markup::Html, body.to_string())],
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{kobo, temp_dir};
    use super::*;

    const ENTRIES: &[(&str, &str)] = &[
        (
            "ça",
            "<html><w><p><a name=\"çay\" /><b>çay</b></p><var><variant name=\"çaylar\"/></var><p>Sıcak içecek.</p></w>\
             <w><p><a name=\"çay\" /><b>çay</b></p><var></var><p>Küçük akarsu.</p></w></html>",
        ),
        (
            "ka",
            "<html><w><p><a name=\"Kâğıt &amp; kalem\" /><b>kâğıt</b></p><var></var>yazı gereci</w></html>",
        ),
    ];

    #[test]
    fn reads_gzipped_and_plain_entries_with_variants() {
        let dir = temp_dir("kobo");
        for gz in [true, false] {
            let path = kobo(&dir, &format!("dicthtml-tr-{gz}.zip"), ENTRIES, gz);
            let d = Kobo::open(&path).unwrap();
            assert_eq!(d.entry_count(), 3);
            let a = d.lookup(&key("ÇAY")).unwrap();
            assert_eq!(a.len(), 2, "eş sesli iki madde");
            assert!(a[0].parts[0].1.contains("Sıcak içecek"));
            assert!(a[1].parts[0].1.contains("Küçük akarsu"));
            assert_eq!(d.lookup(&key("çaylar")).unwrap().len(), 1, "çekimli biçim");
            assert_eq!(
                d.lookup(&key("kağıt & kalem")).unwrap()[0].headword,
                "Kâğıt & kalem"
            );
        }
    }

    #[test]
    fn encrypted_files_are_refused() {
        let dir = temp_dir("kobo-sifreli");
        let path = kobo(
            &dir,
            "dicthtml-tr.zip",
            &[("aa", "\u{7f}\u{1}şifreli baytlar")],
            false,
        );
        assert!(matches!(Kobo::open(&path), Err(DictError::Invalid(m)) if m.contains("encrypted")));
    }
}
