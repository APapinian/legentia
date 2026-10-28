//! ABBYY Lingvo DSL sözlükleri (`.dsl`, `.dsl.dz`; GoldenDict'in ikinci yaygın biçimi).
//!
//! - Kodlama: BOM'a göre UTF-16 LE/BE ya da UTF-8; BOM yoksa ikinci bayt 0 ise UTF-16 LE.
//! - `#NAME "…"` gibi başlık satırları.
//! - Kart: satır başından başlayan bir ya da birkaç madde başı satırı, ardından
//!   girintili (sekme ya da boşluk) içerik satırları.
//! - Madde başında `{…}` dizine girmez, `(…)` seçimliktir (colo(u)r → color,
//!   colour), `\` sonraki karakteri olduğu gibi alır.
//!
//! Hazır dizin yoktur: açılışta dosya bir kez akış hâlinde okunup her kartın
//! içeriğinin bayt aralığı çıkarılır; içerik aramada dosyadan (dictzip ise
//! yalnızca ilgili parçalar açılarak) okunur.

use std::cell::RefCell;
use std::io::{BufReader, Read};
use std::path::Path;

use super::dictzip::Data;
use super::{Article, DictError, Dictionary, Index, MAX_ARTICLE_BYTES, Markup, key};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}

impl Encoding {
    fn unit(self) -> usize {
        if self == Encoding::Utf8 { 1 } else { 2 }
    }

    fn decode(self, bytes: &[u8]) -> String {
        let units = |f: fn([u8; 2]) -> u16| {
            let u: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| f(*c)).collect();
            char::decode_utf16(u)
                .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
                .collect()
        };
        match self {
            Encoding::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
            Encoding::Utf16Le => units(u16::from_le_bytes),
            Encoding::Utf16Be => units(u16::from_be_bytes),
        }
    }
}

/// (kodlama, BOM uzunluğu).
fn detect(head: &[u8]) -> (Encoding, usize) {
    match head {
        [0xff, 0xfe, ..] => (Encoding::Utf16Le, 2),
        [0xfe, 0xff, ..] => (Encoding::Utf16Be, 2),
        [0xef, 0xbb, 0xbf, ..] => (Encoding::Utf8, 3),
        [_, 0, ..] => (Encoding::Utf16Le, 0),
        _ => (Encoding::Utf8, 0),
    }
}

pub struct Dsl {
    name: String,
    encoding: Encoding,
    /// Kartlar: içeriğin bayt aralığı ve madde başının `headwords` içindeki yeri.
    cards: Vec<(u64, u32, u32, u32)>,
    headwords: String,
    index: Index,
    data: RefCell<Data>,
}

/// Madde başı: gösterilecek biçim ve dizin anahtarları.
fn headword(raw: &str) -> (String, Vec<String>) {
    // (gösterilen, seçimliler olmadan, seçimlilerle)
    let (mut shown, mut short, mut long) = (String::new(), String::new(), String::new());
    let (mut unsorted, mut optional) = (false, false);
    let mut chars = raw.trim().chars();
    while let Some(c) = chars.next() {
        let c = match c {
            '\\' => chars.next().unwrap_or('\\'),
            '{' => {
                unsorted = true;
                continue;
            }
            '}' => {
                unsorted = false;
                continue;
            }
            '(' => {
                optional = true;
                continue;
            }
            ')' => {
                optional = false;
                continue;
            }
            c => c,
        };
        shown.push(c);
        if !unsorted {
            long.push(c);
            if !optional {
                short.push(c);
            }
        }
    }
    let mut keys = vec![key(&long)];
    if short != long {
        keys.push(key(&short));
    }
    keys.retain(|k| !k.is_empty());
    (shown.trim().to_string(), keys)
}

impl Dsl {
    pub fn open(path: &Path) -> Result<Dsl, DictError> {
        let file = std::fs::File::open(path)?;
        let compressed = path.extension().is_some_and(|x| x == "dz");
        let reader: Box<dyn Read> = if compressed {
            Box::new(flate2::read::GzDecoder::new(BufReader::new(file)))
        } else {
            Box::new(BufReader::new(file))
        };
        let mut name = None;
        let mut encoding = None;
        let mut cards: Vec<(u64, u32, u32, u32)> = Vec::new();
        let mut headwords = String::new();
        let mut pairs = Vec::new();
        // Okunmakta olan kartın madde başları ve içeriğinin başı/sonu.
        let mut pending: Vec<String> = Vec::new();
        let mut body: Option<(u64, u64)> = None;
        // Başlık satırları yalnızca ilk madde başından öncedir.
        let mut started = false;
        let mut finish = |pending: &mut Vec<String>, body: &mut Option<(u64, u64)>| {
            let Some((start, end)) = body.take() else {
                return;
            };
            for raw in pending.drain(..) {
                let (shown, keys) = headword(&raw);
                let id = cards.len() as u32;
                let len = (end - start).min(MAX_ARTICLE_BYTES as u64) as u32;
                cards.push((start, len, headwords.len() as u32, shown.len() as u32));
                headwords.push_str(&shown);
                pairs.extend(keys.into_iter().map(|k| (k, id)));
            }
        };
        for_each_line(reader, |offset, line, enc| {
            encoding.get_or_insert(enc);
            let text = enc.decode(line);
            let text = text.trim_end_matches(['\r', '\n']);
            if text.trim().is_empty() {
                return;
            }
            if text.starts_with('#') && !started {
                if let Some(v) = text.strip_prefix("#NAME") {
                    name = Some(v.trim().trim_matches('"').to_string());
                }
                return;
            }
            if text.starts_with([' ', '\t']) {
                let end = offset + line.len() as u64;
                body = Some((body.map_or(offset, |b| b.0), end));
            } else {
                if body.is_some() {
                    finish(&mut pending, &mut body);
                }
                pending.push(text.to_string());
                started = true;
            }
        })?;
        finish(&mut pending, &mut body);
        let encoding = encoding.ok_or_else(|| DictError::Invalid("empty dsl".into()))?;
        if cards.is_empty() {
            return Err(DictError::Invalid("no cards in dsl".into()));
        }
        let name = name.filter(|n| !n.is_empty()).unwrap_or_else(|| {
            path.file_name()
                .map(|n| {
                    n.to_string_lossy()
                        .trim_end_matches(".dz")
                        .trim_end_matches(".dsl")
                        .to_string()
                })
                .unwrap_or_default()
        });
        Ok(Dsl {
            name,
            encoding,
            cards,
            headwords,
            index: Index::build(pairs),
            data: RefCell::new(Data::open(path)?),
        })
    }

    pub fn card_count(&self) -> usize {
        self.cards.len()
    }
}

/// Dosyayı akış hâlinde satırlara böler: (açılmış veride satırın konumu, satır
/// baytları (satır sonu dahil), kodlama). Kodlama ilk baytlardan anlaşılır.
fn for_each_line(
    mut reader: Box<dyn Read>,
    mut f: impl FnMut(u64, &[u8], Encoding),
) -> Result<(), DictError> {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    let mut consumed = 0u64; // buf[0]'ın dosyadaki konumu
    let mut enc: Option<(Encoding, usize)> = None;
    let mut eof = false;
    while !eof || !buf.is_empty() {
        if !eof {
            let n = reader.read(&mut chunk)?;
            if n == 0 {
                eof = true;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        let (e, bom) = *enc.get_or_insert_with(|| detect(&buf));
        if consumed == 0 && bom > 0 && buf.len() >= bom {
            buf.drain(..bom);
            consumed = bom as u64;
        }
        let unit = e.unit();
        let mut start = 0;
        let mut i = 0;
        while i + unit <= buf.len() {
            let nl = match e {
                Encoding::Utf8 => buf[i] == b'\n',
                Encoding::Utf16Le => buf[i] == b'\n' && buf[i + 1] == 0,
                Encoding::Utf16Be => buf[i] == 0 && buf[i + 1] == b'\n',
            };
            i += unit;
            if nl {
                f(consumed + start as u64, &buf[start..i], e);
                start = i;
            }
        }
        if eof {
            if start < buf.len() {
                f(consumed + start as u64, &buf[start..], e);
            }
            break;
        }
        buf.drain(..start);
        consumed += start as u64;
    }
    Ok(())
}

impl Dictionary for Dsl {
    fn name(&self) -> &str {
        &self.name
    }

    fn contains(&self, key: &str) -> bool {
        self.index.contains(key)
    }

    fn lookup(&self, key: &str) -> Result<Vec<Article>, DictError> {
        let mut out = Vec::new();
        for id in self.index.find(key) {
            let &(offset, len, hs, hl) = self
                .cards
                .get(id as usize)
                .ok_or_else(|| DictError::Invalid("card".into()))?;
            let bytes = self.data.borrow_mut().read(offset, len)?;
            let text = self.encoding.decode(&bytes);
            let body: Vec<&str> = text
                .lines()
                .map(|l| l.trim_end_matches('\r').trim_start_matches([' ', '\t']))
                .collect();
            out.push(Article {
                headword: self.headwords[hs as usize..(hs + hl) as usize].to_string(),
                parts: vec![(Markup::Dsl, body.join("\n"))],
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{dsl, temp_dir};
    use super::*;

    const SOURCE: &str = "#NAME \"Deneme DSL\"\n#INDEX_LANGUAGE \"Turkish\"\n\nçay\nÇaylar\n\t[m1][b]1.[/b] sıcak içecek[/m]\n\t[m1][ex]demli çay[/ex][/m]\n\ncolo(u)r {renk}\n\t[trn]renk[/trn]\n";

    #[test]
    fn reads_cards_in_every_encoding_and_dictzip() {
        let dir = temp_dir("dsl");
        for (i, (enc, dz)) in [
            (Encoding::Utf16Le, false),
            (Encoding::Utf16Le, true),
            (Encoding::Utf8, false),
            (Encoding::Utf16Be, true),
        ]
        .into_iter()
        .enumerate()
        {
            let path = dsl(
                &dir,
                &format!("d{i}"),
                SOURCE,
                enc == Encoding::Utf8,
                enc == Encoding::Utf16Be,
                dz,
            );
            let d = Dsl::open(&path).unwrap();
            assert_eq!(d.name(), "Deneme DSL");
            assert_eq!(d.card_count(), 3, "{enc:?} {dz}");
            let a = d.lookup(&key("çaylar")).unwrap();
            assert_eq!(a[0].headword, "Çaylar");
            assert_eq!(
                a[0].parts[0].1,
                "[m1][b]1.[/b] sıcak içecek[/m]\n[m1][ex]demli çay[/ex][/m]"
            );
            assert_eq!(d.lookup(&key("çay")).unwrap()[0].headword, "çay");
            for k in ["color", "colour"] {
                let a = d.lookup(&key(k)).unwrap();
                assert_eq!(a[0].headword, "colour renk", "{k}");
                assert_eq!(a[0].parts[0].1, "[trn]renk[/trn]");
            }
            assert!(!d.contains(&key("renk")), "{{…}} dizine girmez");
        }
    }

    #[test]
    fn headword_syntax() {
        assert_eq!(
            headword("colo(u)r"),
            ("colour".into(), vec!["colour".into(), "color".into()])
        );
        assert_eq!(headword("{to }go"), ("to go".into(), vec!["go".into()]));
        assert_eq!(headword("a\\(b\\)"), ("a(b)".into(), vec!["a(b)".into()]));
    }

    #[test]
    fn empty_or_garbage_is_an_error() {
        let dir = temp_dir("dsl-bos");
        let p = dir.join("bos.dsl");
        std::fs::write(&p, b"").unwrap();
        assert!(Dsl::open(&p).is_err());
        std::fs::write(&p, b"#NAME \"x\"\n").unwrap();
        assert!(Dsl::open(&p).is_err());
    }
}
