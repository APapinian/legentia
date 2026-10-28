//! StarDict sözlükleri (GoldenDict ve KOReader'ın ortak biçimi).
//!
//! - `.ifo`: "StarDict's dict ifo file" satırı ve `anahtar=değer` ayarları
//!   (bookname, wordcount, idxoffsetbits, sametypesequence…).
//! - `.idx` (ya da `.idx.gz`): sıralı maddeler; her biri NUL ile biten kelime,
//!   konum (32 ya da 64 bit, büyük sonlu) ve uzunluk (32 bit).
//! - `.syn` (isteğe bağlı): NUL ile biten eş biçim ve `.idx` sırası (32 bit).
//! - `.dict` ya da `.dict.dz`: maddelerin içeriği.
//!
//! İçerik alanları: `sametypesequence` varsa türler oradadır ve alanların önünde
//! tür harfi yoktur; yoksa her alan tür harfiyle başlar. Küçük harfli türler NUL
//! ile biten metin, büyük harfliler 32 bit boyla başlayan veridir; son alanın
//! sonlandırıcısı ya da boyu yoktur.

use std::cell::RefCell;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::dictzip::Data;
use super::{
    Article, DictError, Dictionary, Index, MAX_ARTICLE_BYTES, MAX_INDEX_BYTES, Markup, key,
};

fn invalid(what: impl Into<String>) -> DictError {
    DictError::Invalid(what.into())
}

pub struct StarDict {
    name: String,
    /// `.idx` dosyasının içeriği; madde başı ve konumu buradan okunur.
    idx: Vec<u8>,
    /// Her maddenin `idx` içindeki başlangıcı.
    starts: Vec<u32>,
    offset_bits: u8,
    types: Option<String>,
    index: Index,
    data: RefCell<Data>,
}

/// `path` uzantısını değiştirip var olan ilk dosyayı bulur.
fn sibling(ifo: &Path, exts: &[&str]) -> Option<PathBuf> {
    exts.iter()
        .map(|e| ifo.with_extension(e))
        .find(|p| p.exists())
}

fn read_limited(path: &Path) -> Result<Vec<u8>, DictError> {
    let size = std::fs::metadata(path)?.len();
    if size > MAX_INDEX_BYTES {
        return Err(invalid(format!("{} too large", path.display())));
    }
    let file = std::fs::File::open(path)?;
    let mut out = Vec::new();
    if path.extension().is_some_and(|x| x == "gz") {
        flate2::read::GzDecoder::new(file)
            .take(MAX_INDEX_BYTES)
            .read_to_end(&mut out)?;
    } else {
        file.take(MAX_INDEX_BYTES).read_to_end(&mut out)?;
    }
    Ok(out)
}

impl StarDict {
    pub fn open(ifo: &Path) -> Result<StarDict, DictError> {
        let text = std::fs::read_to_string(ifo)?;
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some("StarDict's dict ifo file") {
            return Err(invalid("not a StarDict .ifo"));
        }
        let setting = |name: &str| {
            text.lines()
                .filter_map(|l| l.split_once('='))
                .find(|(k, _)| k.trim() == name)
                .map(|(_, v)| v.trim().to_string())
        };
        let name = setting("bookname").unwrap_or_else(|| {
            ifo.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        let offset_bits = match setting("idxoffsetbits").as_deref() {
            Some("64") => 64,
            _ => 32,
        };
        let types = setting("sametypesequence").filter(|t| !t.is_empty());

        let idx_path = sibling(ifo, &["idx", "idx.gz"]).ok_or_else(|| invalid("no .idx"))?;
        let idx = read_limited(&idx_path)?;
        let starts = entry_starts(&idx, offset_bits)?;
        let mut pairs: Vec<(String, u32)> = starts
            .iter()
            .enumerate()
            .map(|(i, &s)| (key(&word_at(&idx, s as usize)), i as u32))
            .collect();
        if let Some(syn) = sibling(ifo, &["syn", "syn.gz"]) {
            let syn = read_limited(&syn)?;
            let mut p = 0;
            while p < syn.len() {
                let nul = syn[p..]
                    .iter()
                    .position(|&b| b == 0)
                    .ok_or_else(|| invalid("syn"))?;
                let word = String::from_utf8_lossy(&syn[p..p + nul]);
                let at = p + nul + 1;
                let n = syn.get(at..at + 4).ok_or_else(|| invalid("syn"))?;
                let item = u32::from_be_bytes([n[0], n[1], n[2], n[3]]);
                if (item as usize) < starts.len() {
                    pairs.push((key(&word), item));
                }
                p = at + 4;
            }
        }
        let data_path = sibling(ifo, &["dict", "dict.dz"]).ok_or_else(|| invalid("no .dict"))?;
        Ok(StarDict {
            name,
            idx,
            starts,
            offset_bits,
            types,
            index: Index::build(pairs),
            data: RefCell::new(Data::open(&data_path)?),
        })
    }

    /// Madde sayısı (eş biçimler hariç).
    pub fn word_count(&self) -> usize {
        self.starts.len()
    }

    /// `item`. maddenin başı, konumu ve uzunluğu.
    fn entry(&self, item: u32) -> Option<(String, u64, u32)> {
        let s = *self.starts.get(item as usize)? as usize;
        let word = word_at(&self.idx, s);
        let at = s + self.idx[s..].iter().position(|&b| b == 0)? + 1;
        let (offset, rest) = if self.offset_bits == 64 {
            let b = self.idx.get(at..at + 8)?;
            (u64::from_be_bytes(b.try_into().ok()?), at + 8)
        } else {
            let b = self.idx.get(at..at + 4)?;
            (u32::from_be_bytes(b.try_into().ok()?) as u64, at + 4)
        };
        let b = self.idx.get(rest..rest + 4)?;
        Some((word, offset, u32::from_be_bytes(b.try_into().ok()?)))
    }
}

fn word_at(idx: &[u8], start: usize) -> String {
    let end = idx[start..]
        .iter()
        .position(|&b| b == 0)
        .map_or(idx.len(), |n| start + n);
    String::from_utf8_lossy(&idx[start..end]).into_owned()
}

fn entry_starts(idx: &[u8], offset_bits: u8) -> Result<Vec<u32>, DictError> {
    let tail = if offset_bits == 64 { 12 } else { 8 };
    let mut starts = Vec::new();
    let mut p = 0;
    while p < idx.len() {
        let nul = idx[p..]
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| invalid("idx"))?;
        starts.push(p as u32);
        p += nul + 1 + tail;
        if p > idx.len() {
            return Err(invalid("truncated idx"));
        }
    }
    Ok(starts)
}

fn markup(t: char) -> Option<Markup> {
    match t {
        'm' | 't' | 'y' | 'l' => Some(Markup::Plain),
        'h' => Some(Markup::Html),
        'g' => Some(Markup::Pango),
        'x' => Some(Markup::Xdxf),
        _ => None,
    }
}

/// Madde verisini alanlarına ayırır; görüntülenemeyen türler (ses, resim) atlanır.
fn parse_fields(data: &[u8], types: Option<&str>) -> Vec<(Markup, String)> {
    let mut out = Vec::new();
    let mut p = 0;
    let fixed: Vec<char> = types.map(|t| t.chars().collect()).unwrap_or_default();
    let mut n = 0;
    while p < data.len() {
        let t = if fixed.is_empty() {
            let t = data[p] as char;
            p += 1;
            t
        } else {
            match fixed.get(n) {
                Some(&t) => t,
                None => break,
            }
        };
        let last = !fixed.is_empty() && n + 1 == fixed.len();
        n += 1;
        let field: &[u8] = if last {
            let f = &data[p..];
            p = data.len();
            f
        } else if t.is_ascii_uppercase() {
            let Some(b) = data.get(p..p + 4) else { break };
            let len = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize;
            let Some(f) = data.get(p + 4..p + 4 + len) else {
                break;
            };
            p += 4 + len;
            f
        } else {
            let end = data[p..]
                .iter()
                .position(|&b| b == 0)
                .map_or(data.len(), |e| p + e);
            let f = &data[p..end];
            p = end + 1;
            f
        };
        if let Some(m) = markup(t) {
            out.push((m, String::from_utf8_lossy(field).into_owned()));
        }
    }
    out
}

impl Dictionary for StarDict {
    fn name(&self) -> &str {
        &self.name
    }

    fn contains(&self, key: &str) -> bool {
        self.index.contains(key)
    }

    fn lookup(&self, key: &str) -> Result<Vec<Article>, DictError> {
        let mut out = Vec::new();
        for item in self.index.find(key) {
            let (headword, offset, size) = self.entry(item).ok_or_else(|| invalid("idx entry"))?;
            if size > MAX_ARTICLE_BYTES {
                return Err(invalid("article too large"));
            }
            let data = self.data.borrow_mut().read(offset, size)?;
            out.push(Article {
                headword,
                parts: parse_fields(&data, self.types.as_deref()),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{stardict, temp_dir};
    use super::*;

    #[test]
    fn looks_up_words_case_and_circumflex_insensitively() {
        let dir = temp_dir("sd");
        for dz in [false, true] {
            let ifo = stardict(
                &dir,
                if dz { "dz" } else { "duz" },
                &[
                    ("Kâğıt", "yazı yazılan ince yaprak"),
                    ("ağaç", "gövdeli bitki"),
                    ("İğne", "dikiş aracı"),
                ],
                &[("kağıtlar", 0)],
                dz,
            );
            let d = StarDict::open(&ifo).unwrap();
            assert_eq!(d.word_count(), 3);
            let a = d.lookup(&key("KAĞIT")).unwrap();
            assert_eq!(a.len(), 1);
            assert_eq!(a[0].headword, "Kâğıt");
            assert_eq!(
                a[0].parts,
                vec![(Markup::Plain, "yazı yazılan ince yaprak".to_string())]
            );
            assert_eq!(d.lookup(&key("iğne")).unwrap()[0].headword, "İğne");
            assert!(d.contains(&key("kağıtlar")), "eş biçim (.syn)");
            assert!(d.lookup(&key("yok")).unwrap().is_empty());
        }
    }

    #[test]
    fn fields_without_sametypesequence_carry_their_type() {
        // Html alanı, 2 baylık ses (W, boyla), düz metin.
        let mut v = b"hk<b>bold</b>\0W".to_vec();
        v.extend_from_slice(&[0, 0, 0, 2, b'a', b'b']);
        v.extend_from_slice(b"mson\0");
        let fields = parse_fields(&v, None);
        assert_eq!(
            fields,
            vec![
                (Markup::Html, "k<b>bold</b>".to_string()),
                (Markup::Plain, "son".to_string())
            ]
        );
    }

    #[test]
    fn broken_files_are_errors_not_panics() {
        let dir = temp_dir("sd-bozuk");
        let ifo = stardict(&dir, "b", &[("a", "b")], &[], false);
        let idx = ifo.with_extension("idx");
        let mut bytes = std::fs::read(&idx).unwrap();
        bytes.truncate(bytes.len() - 3);
        std::fs::write(&idx, bytes).unwrap();
        assert!(StarDict::open(&ifo).is_err());
        std::fs::write(&ifo, "rastgele").unwrap();
        assert!(StarDict::open(&ifo).is_err());
    }
}
