//! MOBI (Mobipocket/MOBI6) ve KF8 (AZW3) okuma: açılırken bellekte EPUB'a
//! çevrilir (`convert.rs` ile aynı yol). Kendi kodumuzdur:
//!
//! - PDB kapsayıcısı, MOBI başlığı ve EXTH meta verisi (yazar, başlık, dil,
//!   kapak, KF8 sınırı).
//! - Metin kayıtları: sondaki ek veriler (trailing entries) kırpılır, PalmDOC
//!   (LZ77) açılır. HUFF/CDIC sıkıştırması desteklenmez; DRM'li dosya açılmaz.
//! - MOBI6: `<mbp:pagebreak/>` ile bölünür; `filepos` bağlantıları ve
//!   `recindex` görselleri çözülür.
//! - KF8: FDST'deki ilk akış alınır; her HTML dosyası bir bölüm; görseller
//!   `kindle:embed:XXXX`; iç `kindle:pos` bağlantıları atılır. Birleşik
//!   (MOBI6 + KF8) dosyada KF8 bölümü kullanılır.

use std::collections::HashMap;

use super::EpubError;
use super::convert::{Converted, cp1254, escape};

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn damaged(what: &str) -> EpubError {
    EpubError::Xml {
        entry: "mobi".into(),
        detail: what.into(),
    }
}

/// Palm veritabanı: kayıtların başlangıçları.
struct Pdb<'a> {
    data: &'a [u8],
    offsets: Vec<usize>,
}

impl<'a> Pdb<'a> {
    fn parse(data: &'a [u8]) -> Result<Pdb<'a>, EpubError> {
        if data.get(60..68) != Some(b"BOOKMOBI".as_slice()) {
            return Err(EpubError::NotZip("not a MOBI file".into()));
        }
        let n = be16(data, 76).ok_or_else(|| damaged("header"))? as usize;
        let offsets = (0..n)
            .map(|i| be32(data, 78 + i * 8).map(|o| o as usize))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| damaged("record list"))?;
        if offsets.windows(2).any(|w| w[0] > w[1])
            || offsets.last().is_some_and(|&o| o > data.len())
        {
            return Err(damaged("record offsets"));
        }
        Ok(Pdb { data, offsets })
    }

    fn record(&self, i: usize) -> Option<&'a [u8]> {
        let start = *self.offsets.get(i)?;
        let end = self.offsets.get(i + 1).copied().unwrap_or(self.data.len());
        self.data.get(start..end)
    }

    fn len(&self) -> usize {
        self.offsets.len()
    }
}

/// Bir MOBI bölümünün (record 0) başlığı.
struct Header {
    compression: u16,
    text_len: usize,
    text_records: usize,
    encryption: u16,
    utf8: bool,
    version: u32,
    first_image: Option<usize>,
    extra_flags: u16,
    fdst: Option<usize>,
    title: String,
    exth: Vec<(u32, Vec<u8>)>,
}

fn header(rec0: &[u8]) -> Result<Header, EpubError> {
    let h = |at| be32(rec0, at).ok_or_else(|| damaged("mobi header"));
    if rec0.get(16..20) != Some(b"MOBI".as_slice()) {
        return Err(damaged("no MOBI header"));
    }
    let header_len = h(20)? as usize;
    let utf8 = h(28)? == 65001;
    let version = h(36).unwrap_or(0);
    let index = |v: u32| (v != 0 && v != u32::MAX).then_some(v as usize);
    let first_image = h(108).ok().and_then(index);
    // Ek kayıt bayrakları yalnızca yeterince uzun başlıkta.
    let extra_flags = if 16 + header_len >= 0xF4 {
        be16(rec0, 0xF2).unwrap_or(0)
    } else {
        0
    };
    let fdst = if version >= 8 && 16 + header_len >= 0xC4 {
        be32(rec0, 0xC0).and_then(index)
    } else {
        None
    };
    let mut exth = Vec::new();
    if h(128).unwrap_or(0) & 0x40 != 0 {
        let at = 16 + header_len;
        if rec0.get(at..at + 4) == Some(b"EXTH".as_slice()) {
            let count = be32(rec0, at + 8).unwrap_or(0) as usize;
            let mut p = at + 12;
            for _ in 0..count.min(1024) {
                let (Some(kind), Some(len)) = (be32(rec0, p), be32(rec0, p + 4)) else {
                    break;
                };
                let len = len as usize;
                let Some(data) = rec0.get(p + 8..p + len.max(8)) else {
                    break;
                };
                exth.push((kind, data.to_vec()));
                p += len.max(8);
            }
        }
    }
    let name_at = h(84).unwrap_or(0) as usize;
    let name_len = h(88).unwrap_or(0) as usize;
    let raw_title = rec0.get(name_at..name_at + name_len).unwrap_or_default();
    Ok(Header {
        compression: be16(rec0, 0).unwrap_or(0),
        text_len: h(4)? as usize,
        text_records: be16(rec0, 8).unwrap_or(0) as usize,
        encryption: be16(rec0, 12).unwrap_or(0),
        utf8,
        version,
        first_image,
        extra_flags,
        fdst,
        title: text(raw_title, utf8),
        exth,
    })
}

/// Windows-1252 (eski MOBI'lerin kodlaması): 1254'ten altı harfte ayrılır.
fn cp1252(b: u8) -> char {
    match b {
        0xD0 => 'Ð',
        0xDD => 'Ý',
        0xDE => 'Þ',
        0xF0 => 'ð',
        0xFD => 'ý',
        0xFE => 'þ',
        b => cp1254(b),
    }
}

fn text(bytes: &[u8], utf8: bool) -> String {
    if utf8 {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        bytes.iter().map(|&b| cp1252(b)).collect()
    }
}

/// PalmDOC (LZ77) açma.
pub(super) fn palmdoc(input: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(input.len() * 2);
    let mut i = 0;
    while i < input.len() {
        let c = input[i];
        i += 1;
        match c {
            1..=8 => {
                let end = (i + c as usize).min(input.len());
                out.extend_from_slice(&input[i..end]);
                i = end;
            }
            0x80..=0xBF => {
                let Some(&next) = input.get(i) else { break };
                i += 1;
                let x = (((c as usize) << 8) | next as usize) & 0x3FFF;
                let (dist, len) = (x >> 3, (x & 7) + 3);
                if dist == 0 || dist > out.len() {
                    break;
                }
                let start = out.len() - dist;
                for k in 0..len {
                    out.push(out[start + k]);
                }
            }
            0xC0..=0xFF => {
                out.push(b' ');
                out.push(c ^ 0x80);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Kaydın sonundaki ek veriler (bayraklara göre) atılmış hâli.
fn trim_trailing(data: &[u8], flags: u16) -> &[u8] {
    let mut end = data.len();
    let mut f = flags >> 1;
    while f != 0 {
        if f & 1 != 0 {
            // Sondan geriye değişken uzunluklu sayı: girdinin boyutu.
            let (mut size, mut shift, mut p) = (0usize, 0, end);
            while p > 0 {
                p -= 1;
                let v = data[p];
                size |= ((v & 0x7F) as usize) << shift;
                shift += 7;
                if v & 0x80 != 0 || shift >= 28 {
                    break;
                }
            }
            end = end.saturating_sub(size);
        }
        f >>= 1;
    }
    if flags & 1 != 0 && end > 0 {
        let n = (data[end - 1] & 3) as usize + 1;
        end = end.saturating_sub(n);
    }
    &data[..end]
}

fn image_ext(b: &[u8]) -> Option<(&'static str, &'static str)> {
    if b.starts_with(&[0xFF, 0xD8]) {
        Some(("jpg", "image/jpeg"))
    } else if b.starts_with(b"\x89PNG") {
        Some(("png", "image/png"))
    } else if b.starts_with(b"GIF8") {
        Some(("gif", "image/gif"))
    } else {
        None
    }
}

/// Görsel kayıtları: 1'den başlayan sıra → (ad, MIME, baytlar).
fn images(
    pdb: &Pdb,
    first: Option<usize>,
    base: usize,
) -> HashMap<usize, (String, String, Vec<u8>)> {
    let mut out = HashMap::new();
    let Some(first) = first.map(|f| f + base) else {
        return out;
    };
    for rec in first..pdb.len() {
        let Some(data) = pdb.record(rec) else { break };
        if let Some((ext, mime)) = image_ext(data) {
            let n = rec - first + 1;
            out.insert(
                n,
                (format!("img{n:05}.{ext}"), mime.to_string(), data.to_vec()),
            );
        }
    }
    out
}

/// Büyük/küçük harf duyarsız arama.
fn find_ci(hay: &str, needle: &str, from: usize) -> Option<usize> {
    let lower = hay.get(from..)?.to_ascii_lowercase();
    lower.find(needle).map(|i| from + i)
}

/// `<body …>` öncesini ve `</body>` sonrasını atar.
fn body_of(html: &str) -> &str {
    let start = find_ci(html, "<body", 0)
        .and_then(|i| html[i..].find('>').map(|j| i + j + 1))
        .unwrap_or(0);
    let end = find_ci(html, "</body", start).unwrap_or(html.len());
    &html[start..end]
}

/// Bölümün ilk başlığının (h1–h3) düz metni.
fn first_heading(html: &str) -> Option<String> {
    let at = ["<h1", "<h2", "<h3"]
        .iter()
        .filter_map(|t| find_ci(html, t, 0))
        .min()?;
    let open = at + html[at..].find('>')? + 1;
    let close = open + find_ci(&html[open..], "</h", 0)?;
    let mut s = String::new();
    let mut in_tag = false;
    for c in html[open..close].chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => s.push(c),
            _ => {}
        }
    }
    let s = super::collapse_ws(&s.replace("&amp;", "&").replace("&nbsp;", " "));
    (!s.is_empty()).then_some(s)
}

/// `attr` adlı özniteliğin (tırnaklı ya da tırnaksız) değeri ve tag içindeki yeri.
fn attr_value(tag: &str, attr: &str) -> Option<(usize, usize, String)> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(attr).map(|i| from + i) {
        let before_ok = i == 0 || !lower.as_bytes()[i - 1].is_ascii_alphanumeric();
        let rest = &tag[i + attr.len()..];
        let eq = rest.len() - rest.trim_start().len();
        if before_ok && rest.trim_start().starts_with('=') {
            let v = rest.trim_start()[1..].trim_start();
            let skip = rest.len() - v.len();
            let (value, len) = match v.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    let end = v[1..].find(q)?;
                    (v[1..1 + end].to_string(), end + 2)
                }
                _ => {
                    let end = v
                        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
                        .unwrap_or(v.len());
                    (v[..end].to_string(), end)
                }
            };
            let _ = eq;
            return Some((i, i + attr.len() + skip + len, value));
        }
        from = i + attr.len();
    }
    None
}

/// Her `<tag …>` için `f(tag) -> yeni tag` uygular.
fn rewrite_tags(html: &str, name: &str, mut f: impl FnMut(&str) -> String) -> String {
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;
    let open = format!("<{name}");
    while let Some(i) = find_ci(html, &open, pos) {
        let after = html.as_bytes().get(i + open.len()).copied().unwrap_or(b' ');
        let Some(end) = html[i..].find('>').map(|j| i + j + 1) else {
            break;
        };
        out.push_str(&html[pos..i]);
        if after.is_ascii_alphanumeric() {
            out.push_str(&html[i..end]);
        } else {
            out.push_str(&f(&html[i..end]));
        }
        pos = end;
    }
    out.push_str(&html[pos..]);
    out
}

/// Görsel öğesini paketteki görsele yöneltir; bulunamazsa atar.
fn image_tag(src: Option<&(String, String, Vec<u8>)>) -> String {
    match src {
        Some((name, _, _)) => format!(r#"<img src="images/{}" alt=""/>"#, escape(name)),
        None => String::new(),
    }
}

/// KF8 görsel adresindeki sıra: `kindle:embed:000A?mime=…` (taban 32).
fn kindle_embed(v: &str) -> Option<usize> {
    let code = v.strip_prefix("kindle:embed:")?.split('?').next()?;
    usize::from_str_radix(code, 32).ok()
}

fn exth_text(h: &Header, kind: u32) -> Vec<String> {
    h.exth
        .iter()
        .filter(|(k, _)| *k == kind)
        .map(|(_, v)| text(v, h.utf8).trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn exth_u32(h: &Header, kind: u32) -> Option<u32> {
    h.exth
        .iter()
        .find(|(k, _)| *k == kind)
        .and_then(|(_, v)| be32(v, 0))
}

/// Bölümün (record 0'ı `base`'te) sıkıştırması açılmış metni.
fn raw_text(pdb: &Pdb, h: &Header, base: usize) -> Result<Vec<u8>, EpubError> {
    if h.encryption != 0 {
        return Err(EpubError::Drm);
    }
    let mut out = Vec::with_capacity(h.text_len);
    for i in 1..=h.text_records {
        let rec = pdb.record(base + i).ok_or_else(|| damaged("text record"))?;
        let data = trim_trailing(rec, h.extra_flags);
        match h.compression {
            1 => out.extend_from_slice(data),
            2 => out.extend(palmdoc(data)),
            _ => return Err(EpubError::UnsupportedCompression),
        }
        if out.len() > 64 << 20 {
            return Err(EpubError::TooLarge("mobi text".into()));
        }
    }
    out.truncate(h.text_len);
    Ok(out)
}

/// MOBI/AZW3 baytlarından kitap.
pub(super) fn to_book(data: &[u8], stem: &str) -> Result<Converted, EpubError> {
    let pdb = Pdb::parse(data)?;
    let mut h = header(pdb.record(0).ok_or_else(|| damaged("record 0"))?)?;
    let mut base = 0;
    // Birleşik dosya: KF8 bölümü (EXTH 121) tercih edilir.
    if h.version < 8
        && let Some(b) = exth_u32(&h, 121).map(|b| b as usize)
    {
        let kf8 = [b, b + 1].into_iter().find(|&i| {
            pdb.record(i)
                .is_some_and(|r| r.get(16..20) == Some(b"MOBI".as_slice()))
        });
        if let Some(i) = kf8
            && let Ok(k) = header(pdb.record(i).unwrap_or_default())
        {
            let meta = std::mem::take(&mut h.exth);
            h = k;
            if h.exth.is_empty() {
                h.exth = meta;
            }
            base = i;
        }
    }
    let raw = raw_text(&pdb, &h, base)?;
    let pictures = images(&pdb, h.first_image, base);
    let mut book = Converted {
        title: exth_text(&h, 503)
            .into_iter()
            .next()
            .or_else(|| (!h.title.trim().is_empty()).then(|| h.title.trim().to_string()))
            .unwrap_or_else(|| stem.to_string()),
        authors: exth_text(&h, 100),
        language: exth_text(&h, 524).into_iter().next(),
        identifier: exth_text(&h, 113)
            .into_iter()
            .next()
            .or_else(|| exth_text(&h, 104).into_iter().next()),
        ..Converted::default()
    };
    if h.version >= 8 {
        kf8_chapters(&mut book, &raw, &h, &pdb, base, &pictures);
    } else {
        mobi6_chapters(&mut book, raw, &h, &pictures);
    }
    if book.chapters.is_empty() {
        book.chapters.push((None, String::new()));
    }
    let cover = exth_u32(&h, 201).map(|o| o as usize + 1);
    let mut list: Vec<_> = pictures.into_iter().collect();
    list.sort_by_key(|(n, _)| *n);
    for (n, img) in list {
        if Some(n) == cover {
            book.cover = Some(img.0.clone());
        }
        book.images.push(img);
    }
    Ok(book)
}

fn mobi6_chapters(
    book: &mut Converted,
    mut raw: Vec<u8>,
    h: &Header,
    pictures: &HashMap<usize, (String, String, Vec<u8>)>,
) {
    // filepos hedeflerine çapa: bayt konumundaki (ya da sonraki) etiketin önüne.
    let mut targets: Vec<usize> = Vec::new();
    {
        let s = String::from_utf8_lossy(&raw).to_ascii_lowercase();
        let mut from = 0;
        while let Some(i) = s[from..].find("filepos=").map(|i| from + i) {
            let digits: String = s[i + 8..]
                .chars()
                .skip_while(|c| *c == '"' || *c == '\'')
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(n) = digits.parse::<usize>() {
                targets.push(n);
            }
            from = i + 8;
        }
    }
    targets.sort_unstable();
    targets.dedup();
    for &t in targets.iter().rev() {
        let Some(at) = raw
            .get(t..)
            .and_then(|r| r.iter().position(|&b| b == b'<'))
            .map(|p| t + p)
        else {
            continue;
        };
        let anchor = format!(r#"<a id="fp{t}"></a>"#);
        raw.splice(at..at, anchor.into_bytes());
    }
    let html = text(&raw, h.utf8);
    let html = body_of(&html).to_string();
    let html = rewrite_tags(&html, "img", |tag| {
        let n = attr_value(tag, "recindex").and_then(|(_, _, v)| v.parse::<usize>().ok());
        image_tag(n.and_then(|n| pictures.get(&n)))
    });
    let html = rewrite_tags(&html, "a", |tag| match attr_value(tag, "filepos") {
        Some((a, b, v)) => {
            let n = v.parse::<usize>().unwrap_or(0);
            format!("{}href=\"\u{0}fp{n}\u{0}\"{}", &tag[..a], &tag[b..])
        }
        None => tag.to_string(),
    });
    // Bölümler sayfa sonlarından.
    let mut parts = Vec::new();
    let mut pos = 0;
    while let Some(i) = find_ci(&html, "<mbp:pagebreak", pos) {
        let end = html[i..].find('>').map_or(html.len(), |j| i + j + 1);
        parts.push(html[pos..i].to_string());
        pos = end;
    }
    parts.push(html[pos..].to_string());
    parts.retain(|p| !p.trim().is_empty());
    let mut ids: HashMap<String, usize> = HashMap::new();
    for (i, p) in parts.iter().enumerate() {
        let mut from = 0;
        while let Some(j) = p[from..].find("<a id=\"fp").map(|j| from + j) {
            let id: String = p[j + 7..].chars().take_while(|c| *c != '"').collect();
            ids.insert(id, i);
            from = j + 9;
        }
    }
    for p in parts {
        let title = first_heading(&p);
        let mut out = String::with_capacity(p.len());
        let mut pieces = p.split('\u{0}');
        out.push_str(pieces.next().unwrap_or_default());
        while let (Some(id), Some(rest)) = (pieces.next(), pieces.next()) {
            match ids.get(id) {
                Some(&c) => out.push_str(&format!("c{c}.xhtml#{id}")),
                None => out.push('#'),
            }
            out.push_str(rest);
        }
        book.chapters
            .push((title, strip_ci(&out, "</mbp:pagebreak>")));
    }
    titles_from_contents_page(book);
}

fn kf8_chapters(
    book: &mut Converted,
    raw: &[u8],
    h: &Header,
    pdb: &Pdb,
    base: usize,
    pictures: &HashMap<usize, (String, String, Vec<u8>)>,
) {
    // FDST varsa ilk akış (metin); geri kalanı CSS/SVG.
    let mut flow = raw;
    if let Some(fdst) = h.fdst.and_then(|i| pdb.record(base + i))
        && fdst.starts_with(b"FDST")
        && let (Some(start), Some(end)) = (be32(fdst, 12), be32(fdst, 16))
        && (start as usize) < (end as usize)
        && end as usize <= raw.len()
    {
        flow = &raw[start as usize..end as usize];
    }
    let html = text(flow, h.utf8);
    let html = rewrite_tags(&html, "img", |tag| {
        let n = attr_value(tag, "src").and_then(|(_, _, v)| kindle_embed(&v));
        image_tag(n.and_then(|n| pictures.get(&n)))
    });
    // İç bağlantıların (kindle:pos) karşılığı kurulmaz; bağlantı metni kalır.
    let html = rewrite_tags(&html, "a", |tag| match attr_value(tag, "href") {
        Some((a, b, v)) if v.starts_with("kindle:") => format!("{}{}", &tag[..a], &tag[b..]),
        _ => tag.to_string(),
    });
    let mut starts: Vec<usize> = Vec::new();
    let mut from = 0;
    while let Some(i) = find_ci(&html, "<html", from) {
        starts.push(i);
        from = i + 5;
    }
    if starts.is_empty() {
        starts.push(0);
    }
    starts.push(html.len());
    for w in starts.windows(2) {
        // İskeletteki yer tutucuların içeriği (parçalar) iskeletin `</html>`'inden
        // sonra gelir; iskeletle birlikte, sırasıyla alınır.
        let file = &html[w[0]..w[1]];
        let start = find_ci(file, "<body", 0)
            .and_then(|i| file[i..].find('>').map(|j| i + j + 1))
            .unwrap_or(0);
        let part = strip_ci(&strip_ci(&file[start..], "</body>"), "</html>");
        if part.trim().is_empty() {
            continue;
        }
        book.chapters.push((first_heading(&part), part));
    }
}

/// `needle`'ın (küçük harfli) bütün geçişleri atılmış metin.
fn strip_ci(s: &str, needle: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pos = 0;
    while let Some(i) = find_ci(s, needle, pos) {
        out.push_str(&s[pos..i]);
        pos = i + needle.len();
    }
    out.push_str(&s[pos..]);
    out
}

/// Başlığı bulunamayan bölümlere kitabın kendi içindekiler sayfasından ad:
/// bölümlere en çok bağlantı veren sayfanın bağlantı metinleri.
fn titles_from_contents_page(book: &mut Converted) {
    let links = |body: &str| -> Vec<(usize, String)> {
        let mut out = Vec::new();
        let mut pos = 0;
        while let Some(i) = body[pos..].find("<a href=\"c").map(|i| pos + i) {
            let rest = &body[i + 10..];
            let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            let Some(open) = rest.find('>') else { break };
            let Some(close) = find_ci(rest, "</a>", open) else {
                break;
            };
            let mut text = String::new();
            let mut in_tag = false;
            for c in rest[open + 1..close].chars() {
                match c {
                    '<' => in_tag = true,
                    '>' => in_tag = false,
                    c if !in_tag => text.push(c),
                    _ => {}
                }
            }
            if let Ok(n) = n.parse::<usize>() {
                out.push((n, super::collapse_ws(&text)));
            }
            pos = i + 10;
        }
        out
    };
    let best = (0..book.chapters.len())
        .map(|i| (i, links(&book.chapters[i].1)))
        .max_by_key(|(_, l)| l.len());
    let Some((page, list)) = best.filter(|(_, l)| l.len() >= 3) else {
        return;
    };
    for (n, text) in list {
        if n != page
            && !text.is_empty()
            && let Some(ch) = book.chapters.get_mut(n)
            && ch.0.is_none()
        {
            ch.0 = Some(text);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// PalmDOC sıkıştırması (yalnızca test için, basit): tekrarları ve
    /// boşluk+harf çiftlerini kullanır.
    fn compress(input: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < input.len() {
            // En uzun geri gönderme (3–10 bayt, en çok 2047 geride).
            let mut best = (0, 0);
            let start = i.saturating_sub(2047);
            for j in start..i {
                let mut l = 0;
                while l < 10 && i + l < input.len() && input[j + l] == input[i + l] {
                    l += 1;
                }
                if l > best.1 {
                    best = (i - j, l);
                }
            }
            if best.1 >= 3 {
                let x = 0x8000 | (best.0 << 3) | (best.1 - 3);
                out.push((x >> 8) as u8);
                out.push(x as u8);
                i += best.1;
            } else if input[i] == b' '
                && i + 1 < input.len()
                && (0x40..0x80).contains(&input[i + 1])
            {
                out.push(input[i + 1] ^ 0x80);
                i += 2;
            } else if (0x09..0x80).contains(&input[i]) || input[i] == 0 {
                out.push(input[i]);
                i += 1;
            } else {
                out.push(1);
                out.push(input[i]);
                i += 1;
            }
        }
        out
    }

    /// Test için MOBI6 ya da KF8 dosyası: metin, görseller, EXTH.
    pub(crate) fn build(
        html: &str,
        kf8: bool,
        images: &[Vec<u8>],
        exth: &[(u32, &[u8])],
    ) -> Vec<u8> {
        let text = html.as_bytes();
        let chunks: Vec<&[u8]> = text.chunks(4096).collect();
        let mut records: Vec<Vec<u8>> = Vec::new();
        let mut exth_bytes = Vec::new();
        for (k, v) in exth {
            exth_bytes.extend(k.to_be_bytes());
            exth_bytes.extend(((v.len() + 8) as u32).to_be_bytes());
            exth_bytes.extend_from_slice(v);
        }
        let header_len: u32 = 0xE8;
        let mut r0 = vec![0u8; 16 + header_len as usize];
        r0[0..2].copy_from_slice(&2u16.to_be_bytes());
        r0[4..8].copy_from_slice(&(text.len() as u32).to_be_bytes());
        r0[8..10].copy_from_slice(&(chunks.len() as u16).to_be_bytes());
        r0[10..12].copy_from_slice(&4096u16.to_be_bytes());
        r0[16..20].copy_from_slice(b"MOBI");
        r0[20..24].copy_from_slice(&header_len.to_be_bytes());
        r0[28..32].copy_from_slice(&65001u32.to_be_bytes());
        r0[36..40].copy_from_slice(&(if kf8 { 8u32 } else { 6 }).to_be_bytes());
        let first_image = chunks.len() as u32 + 1;
        r0[108..112].copy_from_slice(&first_image.to_be_bytes());
        r0[128..132].copy_from_slice(&0x40u32.to_be_bytes());
        // Ek veri: çok baytlı karakter bayrağı (her kayda bir 0 baytı eklenir).
        r0[0xF2..0xF4].copy_from_slice(&1u16.to_be_bytes());
        if kf8 {
            r0[0xC0..0xC4].copy_from_slice(&u32::MAX.to_be_bytes());
        }
        r0.extend(b"EXTH");
        r0.extend(((12 + exth_bytes.len()) as u32).to_be_bytes());
        r0.extend((exth.len() as u32).to_be_bytes());
        r0.extend(exth_bytes);
        let name_at = r0.len() as u32;
        r0.extend(b"Kitap Adi");
        r0[84..88].copy_from_slice(&name_at.to_be_bytes());
        r0[88..92].copy_from_slice(&9u32.to_be_bytes());
        records.push(r0);
        for c in &chunks {
            let mut r = compress(c);
            r.push(0); // çok baytlı ek: 0 → 1 bayt atılır
            records.push(r);
        }
        records.extend(images.iter().cloned());
        let mut out = vec![0u8; 78 + records.len() * 8 + 2];
        out[60..68].copy_from_slice(b"BOOKMOBI");
        out[76..78].copy_from_slice(&(records.len() as u16).to_be_bytes());
        let mut at = out.len();
        for (i, r) in records.iter().enumerate() {
            out[78 + i * 8..82 + i * 8].copy_from_slice(&(at as u32).to_be_bytes());
            at += r.len();
        }
        for r in records {
            out.extend(r);
        }
        out
    }

    #[test]
    fn palmdoc_round_trips_and_trailing_entries_are_trimmed() {
        let s = b"Ali ata bak, ata bak ata bak! \xc3\xa7ay \x01\x7f".to_vec();
        assert_eq!(palmdoc(&compress(&s)), s);
        // Bayrak 0b11: önce 2 baytlık ek girdi (boyutu son baytta, 0x82),
        // sonra çok baytlı ek (son baytın düşük iki biti + 1).
        let data = [b'a', b'b', b'c', 0x01, b'x', 0x82];
        assert_eq!(trim_trailing(&data, 0b11), b"ab");
        assert_eq!(kindle_embed("kindle:embed:000A?mime=image/jpeg"), Some(10));
    }

    #[test]
    fn mobi6_book_has_chapters_links_images_and_metadata() {
        let png = crate::epub::testutil::png(10, 15, 30);
        let html = "<html><head><title>x</title></head><body><h1>Birinci Bölüm</h1><p>Metin <a filepos={FP}>git</a>.</p>\
             <p><img recindex=\"00001\"></p><mbp:pagebreak/><h2>İkinci</h2><p>Hedef burada.</p></body></html>"
            .to_string();
        // filepos: ikinci bölümdeki <p>'nin bayt konumu.
        let target = html.find("<p>Hedef").unwrap();
        let fp = format!("{target:010}");
        let html = html.replace("{FP}", &fp);
        let shift = fp.len() as isize - 4;
        let target = (target as isize + shift) as usize;
        let html = html.replace(&fp, &format!("{target:010}"));
        let data = build(
            &html,
            false,
            &[png],
            &[
                (100, "Yazar Bir".as_bytes()),
                (524, b"tr"),
                (201, &0u32.to_be_bytes()),
            ],
        );
        let book = to_book(&data, "dosya").unwrap();
        assert_eq!(book.title, "Kitap Adi");
        assert_eq!(book.authors, ["Yazar Bir"]);
        assert_eq!(book.language.as_deref(), Some("tr"));
        assert_eq!(book.chapters.len(), 2);
        assert_eq!(book.chapters[0].0.as_deref(), Some("Birinci Bölüm"));
        assert!(
            book.chapters[0].1.contains(r#"href="c1.xhtml#fp"#),
            "{}",
            book.chapters[0].1
        );
        assert!(
            book.chapters[0]
                .1
                .contains(r#"<img src="images/img00001.png""#)
        );
        assert!(book.chapters[1].1.contains("<a id=\"fp"));
        assert_eq!(book.cover.as_deref(), Some("img00001.png"));
    }

    #[test]
    fn kf8_book_splits_files_and_resolves_embedded_images() {
        let png = crate::epub::testutil::png(10, 15, 30);
        let html = "<?xml version=\"1.0\"?><html><body><h1>Bir</h1><p>Önsöz <a href=\"kindle:pos:fid:0001:off:0000000000\">bağ</a></p>\
            <img src=\"kindle:embed:0001?mime=image/png\"/></body></html>\
            <html><body><h1>İki</h1><p>Son.</p></body></html>";
        let data = build(html, true, &[png], &[(503, "Güncel Ad".as_bytes())]);
        let book = to_book(&data, "dosya").unwrap();
        assert_eq!(book.title, "Güncel Ad");
        let titles: Vec<_> = book
            .chapters
            .iter()
            .map(|c| c.0.clone().unwrap_or_default())
            .collect();
        assert_eq!(titles, ["Bir", "İki"]);
        assert!(book.chapters[0].1.contains("images/img00001.png"));
        assert!(!book.chapters[0].1.contains("kindle:pos"));
    }

    #[test]
    fn drm_and_non_mobi_files_are_refused() {
        let mut data = build("<p>x</p>", false, &[], &[]);
        let r0 = u32::from_be_bytes(data[78..82].try_into().unwrap()) as usize;
        data[r0 + 12] = 0;
        data[r0 + 13] = 2;
        assert!(matches!(to_book(&data, "x"), Err(EpubError::Drm)));
        assert!(to_book(b"PK\x03\x04", "x").is_err());
    }
}
