//! MDict sözlükleri (`.mdx`; GoldenDict'te özellikle büyük sözlükler).
//!
//! Yapı (sürüm 2.0'da sayılar 8, 1.2'de 4 bayt, büyük sonlu):
//! - Başlık: 4 bayt boy, UTF-16 LE XML (`<Dictionary GeneratedByEngineVersion=…
//!   Encoding=… Encrypted=…>`), 4 bayt adler32.
//! - Anahtar bölümü: blok sayısı, madde sayısı, (2.0'da) açılmış blok bilgisi
//!   boyu, blok bilgisi boyu, anahtar blokları boyu, (2.0'da) adler32; blok
//!   bilgisi (2.0'da zlib'li; "Encrypted & 2" ise RIPEMD-128 türetmeli karıştırma);
//!   anahtar blokları: her madde için kayıt konumu ve NUL ile biten anahtar.
//! - Kayıt bölümü: blok sayısı, madde sayısı, bilgi boyu, bloklar boyu; her blok
//!   için (sıkıştırılmış, açılmış) boy; bloklar. Kayıt, açılmış kayıt akışında
//!   anahtarın konumundan bir sonraki kayda kadardır.
//! - Sıkıştırılmış blok: 4 bayt tür (0 yok, 1 LZO, 2 zlib), 4 bayt adler32, veri.
//!
//! Desteklenmeyenler: "Encrypted & 1" (satın alınmış sözlüklerde kullanıcıya özel
//! kayıt kodu; kopya korumasıdır), LZO sıkıştırması, GBK/Big5 kodlaması; hepsi
//! açıklayıcı hata verir. Kaynak: github.com/zhansliu/writemdict (fileformat.md), readmdict.

use std::cell::RefCell;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use ripemd::{Digest, Ripemd128};

use super::{
    Article, DictError, Dictionary, Index, MAX_ARTICLE_BYTES, MAX_INDEX_BYTES, Markup, key,
};

fn invalid(what: impl Into<String>) -> DictError {
    DictError::Invalid(format!("mdict: {}", what.into()))
}

/// `@@@LINK=` yönlendirmelerinin en fazla kaç kez izleneceği.
const MAX_LINKS: usize = 3;

pub struct MDict {
    name: String,
    utf16: bool,
    /// Madde başları ve kayıt konumları, anahtar bloklarındaki sırayla.
    headwords: Vec<String>,
    offsets: Vec<u64>,
    /// Bütün kayıt konumları, sıralı; bir kaydın sonu sonraki konumdur.
    sorted_offsets: Vec<u64>,
    /// Kayıt blokları: (dosyadaki yer, sıkıştırılmış boy, açılmış akıştaki başı, açılmış boy).
    blocks: Vec<(u64, u64, u64, u64)>,
    index: Index,
    file: RefCell<File>,
}

/// Bayt dizisinden sırayla okuyucu.
struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], DictError> {
        let end = self.at.checked_add(n).ok_or_else(|| invalid("size"))?;
        let s = self
            .data
            .get(self.at..end)
            .ok_or_else(|| invalid("truncated"))?;
        self.at = end;
        Ok(s)
    }

    /// `width` (1, 2, 4 ya da 8) baytlık büyük sonlu sayı.
    fn number(&mut self, width: usize) -> Result<u64, DictError> {
        Ok(self
            .take(width)?
            .iter()
            .fold(0u64, |n, &b| (n << 8) | b as u64))
    }
}

fn read_at(file: &mut File, at: u64, len: u64) -> Result<Vec<u8>, DictError> {
    if len > MAX_INDEX_BYTES {
        return Err(invalid("block too large"));
    }
    file.seek(SeekFrom::Start(at))?;
    let mut buf = vec![0u8; len as usize];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

/// Sıkıştırılmış bloğu açar (en fazla `limit` bayt).
fn unpack(block: &[u8], limit: u64) -> Result<Vec<u8>, DictError> {
    let kind = block.get(..4).ok_or_else(|| invalid("block header"))?;
    let data = block.get(8..).ok_or_else(|| invalid("block header"))?;
    match kind {
        [0, 0, 0, 0] => Ok(data.to_vec()),
        [2, 0, 0, 0] => {
            let mut out = Vec::new();
            flate2::read::ZlibDecoder::new(data)
                .take(limit.min(MAX_INDEX_BYTES))
                .read_to_end(&mut out)?;
            Ok(out)
        }
        [1, 0, 0, 0] => Err(invalid("LZO compression is not supported")),
        _ => Err(invalid("unknown compression")),
    }
}

/// Anahtar bloğu bilgisinin karıştırmasını çözer ("Encrypted & 2").
fn descramble(block: &mut [u8]) {
    if block.len() < 8 {
        return;
    }
    let mut h = Ripemd128::new();
    h.update(&block[4..8]);
    h.update(0x3695u32.to_le_bytes());
    let key = h.finalize();
    let mut prev = 0x36u8;
    for (i, b) in block[8..].iter_mut().enumerate() {
        let c = *b;
        *b = c.rotate_left(4) ^ prev ^ (i as u8) ^ key[i % key.len()];
        prev = c;
    }
}

/// Başlık XML'inde `name="…"` değeri.
fn header_attr(header: &str, name: &str) -> Option<String> {
    let at = header.find(&format!(" {name}=\""))? + name.len() + 3;
    let end = header[at..].find('"')? + at;
    Some(super::render::entities(&header[at..end]))
}

impl MDict {
    pub fn open(path: &Path) -> Result<MDict, DictError> {
        let mut file = File::open(path)?;
        let size = file.metadata()?.len();
        let header_len =
            u32::from_be_bytes(read_at(&mut file, 0, 4)?.try_into().expect("4 bayt")) as u64;
        let header_bytes = read_at(&mut file, 4, header_len.min(size))?;
        let units: Vec<u16> = header_bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        let header = String::from_utf16_lossy(&units);
        let version: f32 = header_attr(&header, "GeneratedByEngineVersion")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(2.0);
        let v2 = version >= 2.0;
        let width = if v2 { 8 } else { 4 };
        let encrypted: u32 = match header_attr(&header, "Encrypted").as_deref() {
            Some("Yes") => 1,
            Some(v) => v.trim().parse().unwrap_or(0),
            None => 0,
        };
        if encrypted & 1 != 0 {
            return Err(invalid(
                "registration-encrypted dictionary is not supported",
            ));
        }
        let encoding = header_attr(&header, "Encoding")
            .unwrap_or_default()
            .to_ascii_uppercase();
        let utf16 = match encoding.as_str() {
            "" | "UTF-8" | "UTF8" => false,
            "UTF-16" | "UTF16" => true,
            other => return Err(invalid(format!("encoding {other} is not supported"))),
        };
        let name = header_attr(&header, "Title")
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty() && !t.starts_with("Title"))
            .unwrap_or_else(|| {
                path.file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });

        // Anahtar bölümü.
        let mut at = 4 + header_len + 4;
        let head_len = if v2 { 5 * 8 + 4 } else { 4 * 4 };
        let head = read_at(&mut file, at, head_len)?;
        at += head_len;
        let mut c = Cursor { data: &head, at: 0 };
        let key_blocks = c.number(width)?;
        let _entries = c.number(width)?;
        let info_plain = if v2 { c.number(width)? } else { 0 };
        let info_len = c.number(width)?;
        let blocks_len = c.number(width)?;
        let mut info = read_at(&mut file, at, info_len)?;
        at += info_len;
        let info = if v2 {
            if encrypted & 2 != 0 {
                descramble(&mut info);
            }
            unpack(&info, info_plain.max(1))?
        } else {
            info
        };
        let mut c = Cursor { data: &info, at: 0 };
        let (size_width, term) = if v2 { (2, 1) } else { (1, 0) };
        let unit = if utf16 { 2 } else { 1 };
        let mut key_block_sizes = Vec::new();
        for _ in 0..key_blocks {
            c.number(width)?;
            for _ in 0..2 {
                let n = c.number(size_width)? as usize;
                c.take((n + term) * unit)?;
            }
            key_block_sizes.push((c.number(width)?, c.number(width)?));
        }
        let key_data = read_at(&mut file, at, blocks_len)?;
        at += blocks_len;
        let mut headwords = Vec::new();
        let mut offsets = Vec::new();
        let mut kc = Cursor {
            data: &key_data,
            at: 0,
        };
        for (comp, plain) in key_block_sizes {
            let block = unpack(kc.take(comp as usize)?, plain)?;
            let mut b = Cursor {
                data: &block,
                at: 0,
            };
            while b.at < block.len() {
                offsets.push(b.number(width)?);
                let rest = &block[b.at..];
                let end = if utf16 {
                    rest.as_chunks::<2>()
                        .0
                        .iter()
                        .position(|u| *u == [0, 0])
                        .map(|i| i * 2)
                } else {
                    rest.iter().position(|&x| x == 0)
                }
                .ok_or_else(|| invalid("key terminator"))?;
                headwords.push(decode(&rest[..end], utf16));
                b.at += end + unit;
            }
        }

        // Kayıt bölümü.
        let head = read_at(&mut file, at, 4 * width as u64)?;
        at += 4 * width as u64;
        let mut c = Cursor { data: &head, at: 0 };
        let record_blocks = c.number(width)?;
        let _entries = c.number(width)?;
        let rinfo_len = c.number(width)?;
        let _rblocks_len = c.number(width)?;
        let rinfo = read_at(&mut file, at, rinfo_len)?;
        at += rinfo_len;
        let mut c = Cursor {
            data: &rinfo,
            at: 0,
        };
        let mut blocks = Vec::new();
        let mut plain_at = 0u64;
        for _ in 0..record_blocks {
            let (comp, plain) = (c.number(width)?, c.number(width)?);
            blocks.push((at, comp, plain_at, plain));
            at += comp;
            plain_at += plain;
        }
        if at > size {
            return Err(invalid("truncated records"));
        }
        if headwords.is_empty() {
            return Err(invalid("no entries"));
        }
        let mut sorted_offsets = offsets.clone();
        sorted_offsets.sort_unstable();
        sorted_offsets.dedup();
        sorted_offsets.push(plain_at);
        let index = Index::build(
            headwords
                .iter()
                .enumerate()
                .map(|(i, h)| (key(h), i as u32)),
        );
        Ok(MDict {
            name,
            utf16,
            headwords,
            offsets,
            sorted_offsets,
            blocks,
            index,
            file: RefCell::new(file),
        })
    }

    pub fn entry_count(&self) -> usize {
        self.headwords.len()
    }

    /// `item`. maddenin kaydı (sondaki NUL'lar atılmış).
    fn record(&self, item: usize) -> Result<String, DictError> {
        let start = self.offsets[item];
        let next = self.sorted_offsets[self
            .sorted_offsets
            .partition_point(|&o| o <= start)
            .min(self.sorted_offsets.len() - 1)];
        let bi = self
            .blocks
            .partition_point(|b| b.2 + b.3 <= start)
            .min(self.blocks.len().saturating_sub(1));
        let &(file_at, comp, plain_at, plain) =
            self.blocks.get(bi).ok_or_else(|| invalid("record block"))?;
        let raw = read_at(&mut self.file.borrow_mut(), file_at, comp)?;
        let block = unpack(&raw, plain)?;
        let from = (start - plain_at) as usize;
        let to = (next.min(plain_at + plain).max(start) - plain_at) as usize;
        if to - from > MAX_ARTICLE_BYTES as usize {
            return Err(invalid("record too large"));
        }
        let bytes = block.get(from..to).ok_or_else(|| invalid("record range"))?;
        Ok(decode(bytes, self.utf16).trim_end_matches('\0').to_string())
    }
}

fn decode(bytes: &[u8], utf16: bool) -> String {
    if utf16 {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

impl Dictionary for MDict {
    fn name(&self) -> &str {
        &self.name
    }

    fn contains(&self, key: &str) -> bool {
        self.index.contains(key)
    }

    fn lookup(&self, k: &str) -> Result<Vec<Article>, DictError> {
        let mut out = Vec::new();
        for item in self.index.find(k) {
            let mut item = item as usize;
            let mut body = self.record(item)?;
            // "@@@LINK=hedef": başka maddeye yönlendirme.
            for _ in 0..MAX_LINKS {
                let Some(target) = body.trim().strip_prefix("@@@LINK=") else {
                    break;
                };
                // Şapkasız anahtar birden çok madde verebilir; yazımı birebir
                // tutan hedef, yoksa kendisi olmayan ilk madde.
                let found = self.index.find(&key(target));
                let Some(&t) = found
                    .iter()
                    .find(|&&t| self.headwords[t as usize] == target.trim())
                    .or_else(|| found.iter().find(|&&t| t as usize != item))
                else {
                    break;
                };
                item = t as usize;
                body = self.record(item)?;
            }
            out.push(Article {
                headword: self.headwords[item].clone(),
                parts: vec![(Markup::Html, body)],
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{mdx, temp_dir};
    use super::*;

    const WORDS: &[(&str, &str)] = &[
        ("ağaç", "<b>isim</b> Gövdeli bitki."),
        ("çay", "Sıcak içecek."),
        ("kâğıt", "Yazı yazılan yaprak."),
        ("kağıt", "@@@LINK=kâğıt"),
        ("zeytin", "Bir meyve."),
    ];

    #[test]
    fn reads_v2_and_v1_files_plain_and_scrambled() {
        let dir = temp_dir("mdx");
        for (i, (v2, scrambled, utf16)) in [
            (true, false, false),
            (true, true, false),
            (false, false, false),
            (true, true, true),
        ]
        .into_iter()
        .enumerate()
        {
            let path = mdx(&dir, &format!("s{i}"), WORDS, v2, scrambled, utf16);
            let d = MDict::open(&path).unwrap_or_else(|e| panic!("{i}: {e:?}"));
            assert_eq!(d.name(), "Deneme MDX");
            assert_eq!(d.entry_count(), 5);
            let a = d.lookup(&key("Ağaç")).unwrap();
            assert_eq!(a[0].parts[0].1, "<b>isim</b> Gövdeli bitki.", "{i}");
            assert_eq!(
                d.lookup(&key("zeytin")).unwrap()[0].parts[0].1,
                "Bir meyve."
            );
            let linked = d.lookup(&key("çay")).unwrap();
            assert_eq!(linked[0].parts[0].1, "Sıcak içecek.");
            // "kağıt" ve "kâğıt" aynı anahtar: ikisi de asıl maddeye çıkar.
            let k = d.lookup(&key("kağıt")).unwrap();
            assert!(
                k.iter().all(|a| a.parts[0].1 == "Yazı yazılan yaprak."),
                "{k:?}"
            );
        }
    }

    #[test]
    fn registration_encrypted_and_garbage_files_are_errors() {
        let dir = temp_dir("mdx-bozuk");
        let path = mdx(&dir, "kayitli", WORDS, true, false, false);
        let mut bytes = std::fs::read(&path).unwrap();
        // Başlıktaki Encrypted="0" → "1" (UTF-16 LE).
        let needle: Vec<u8> = "Encrypted=\"0\""
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let at = bytes
            .windows(needle.len())
            .position(|w| w == needle)
            .unwrap();
        bytes[at + needle.len() - 4] = b'1';
        std::fs::write(&path, &bytes).unwrap();
        assert!(
            matches!(MDict::open(&path), Err(DictError::Invalid(m)) if m.contains("registration"))
        );
        let garbage = dir.join("cop.mdx");
        std::fs::write(&garbage, b"\x00\x00\x00\x10kisa").unwrap();
        assert!(MDict::open(&garbage).is_err());
    }
}
