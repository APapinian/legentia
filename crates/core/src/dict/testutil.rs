//! Testler için küçük sözlük dosyaları üretir.

use std::path::{Path, PathBuf};

pub fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "legentia-dict-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `sametypesequence=m` StarDict sözlüğü; `.ifo` yolunu döner. `syn`: (biçim, madde sırası).
pub fn stardict(
    dir: &Path,
    name: &str,
    words: &[(&str, &str)],
    syn: &[(&str, u32)],
    dictzip: bool,
) -> PathBuf {
    let mut sorted: Vec<_> = words.to_vec();
    sorted.sort_by_key(|(w, _)| w.to_lowercase());
    let mut idx = Vec::new();
    let mut data = Vec::new();
    for (w, def) in &sorted {
        idx.extend_from_slice(w.as_bytes());
        idx.push(0);
        idx.extend_from_slice(&(data.len() as u32).to_be_bytes());
        idx.extend_from_slice(&(def.len() as u32).to_be_bytes());
        data.extend_from_slice(def.as_bytes());
    }
    let ifo = dir.join(format!("{name}.ifo"));
    std::fs::write(
        &ifo,
        format!(
            "StarDict's dict ifo file\nversion=2.4.2\nbookname={name} sözlüğü\nwordcount={}\nidxfilesize={}\nsametypesequence=m\n",
            sorted.len(),
            idx.len()
        ),
    )
    .unwrap();
    std::fs::write(ifo.with_extension("idx"), idx).unwrap();
    if !syn.is_empty() {
        let mut s = Vec::new();
        for (w, i) in syn {
            s.extend_from_slice(w.as_bytes());
            s.push(0);
            s.extend_from_slice(&i.to_be_bytes());
        }
        std::fs::write(ifo.with_extension("syn"), s).unwrap();
    }
    if dictzip {
        std::fs::write(
            ifo.with_extension("dict.dz"),
            super::dictzip::compress(&data, 16),
        )
        .unwrap();
    } else {
        std::fs::write(ifo.with_extension("dict"), data).unwrap();
    }
    ifo
}

/// DSL sözlüğü yazar: varsayılan UTF-16 LE (BOM'lu); `utf8` ya da `be` seçilebilir;
/// `dictzip` ise `.dsl.dz`. Yolu döner.
pub fn dsl(dir: &Path, name: &str, source: &str, utf8: bool, be: bool, dictzip: bool) -> PathBuf {
    let bytes: Vec<u8> = if utf8 {
        [&[0xef, 0xbb, 0xbf][..], source.as_bytes()].concat()
    } else if be {
        [0xfe, 0xff]
            .into_iter()
            .chain(source.encode_utf16().flat_map(u16::to_be_bytes))
            .collect()
    } else {
        [0xff, 0xfe]
            .into_iter()
            .chain(source.encode_utf16().flat_map(u16::to_le_bytes))
            .collect()
    };
    if dictzip {
        let path = dir.join(format!("{name}.dsl.dz"));
        std::fs::write(&path, super::dictzip::compress(&bytes, 64)).unwrap();
        path
    } else {
        let path = dir.join(format!("{name}.dsl"));
        std::fs::write(&path, bytes).unwrap();
        path
    }
}

/// Kobo dicthtml zip'i: (önek, HTML) dosyaları; `gz` ise dosyalar gzip'li.
pub fn kobo(dir: &Path, name: &str, files: &[(&str, &str)], gz: bool) -> PathBuf {
    use std::io::Write;
    let path = dir.join(name);
    let mut z = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (prefix, html) in files {
        z.start_file(format!("{prefix}.html"), opts).unwrap();
        if gz {
            let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(html.as_bytes()).unwrap();
            z.write_all(&e.finish().unwrap()).unwrap();
        } else {
            z.write_all(html.as_bytes()).unwrap();
        }
    }
    z.start_file("words", opts).unwrap();
    z.finish().unwrap();
    path
}

/// MDict `.mdx` yazar (readmdict'in okuduğu yapı): `v2` sürüm 2.0 (8 baytlık
/// sayılar, zlib'li blok bilgisi), değilse 1.2; `scrambled` "Encrypted=2";
/// `utf16` UTF-16 LE kodlama. İki anahtar ve iki kayıt bloğuna böler.
pub fn mdx(
    dir: &Path,
    name: &str,
    words: &[(&str, &str)],
    v2: bool,
    scrambled: bool,
    utf16: bool,
) -> PathBuf {
    use std::io::Write;
    let width = if v2 { 8 } else { 4 };
    let num = |n: u64| -> Vec<u8> {
        if v2 {
            n.to_be_bytes().to_vec()
        } else {
            (n as u32).to_be_bytes().to_vec()
        }
    };
    let enc = |s: &str| -> Vec<u8> {
        if utf16 {
            s.encode_utf16().flat_map(u16::to_le_bytes).collect()
        } else {
            s.as_bytes().to_vec()
        }
    };
    let nul: Vec<u8> = if utf16 { vec![0, 0] } else { vec![0] };
    let zlib = |data: &[u8]| -> Vec<u8> {
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(data).unwrap();
        let mut out = vec![2, 0, 0, 0, 0, 0, 0, 0];
        out.extend(e.finish().unwrap());
        out
    };
    let mut sorted = words.to_vec();
    sorted.sort_by_key(|(w, _)| w.to_string());
    // Kayıtlar ve konumları.
    let mut records = Vec::new();
    let mut offsets = Vec::new();
    for (_, body) in &sorted {
        offsets.push(records.len() as u64);
        records.extend(enc(body));
        records.extend(&nul);
    }
    let halves = [&sorted[..sorted.len() / 2], &sorted[sorted.len() / 2..]];
    // Anahtar blokları ve bilgileri.
    let mut key_blocks = Vec::new();
    let mut info = Vec::new();
    let mut i = 0;
    for half in halves {
        let mut block = Vec::new();
        for (w, _) in half.iter() {
            block.extend(num(offsets[i]));
            block.extend(enc(w));
            block.extend(&nul);
            i += 1;
        }
        let packed = zlib(&block);
        info.extend(num(half.len() as u64));
        for w in [half[0].0, half[half.len() - 1].0] {
            let units = if utf16 {
                w.encode_utf16().count()
            } else {
                w.len()
            };
            if v2 {
                info.extend((units as u16).to_be_bytes());
                info.extend(enc(w));
                info.extend(&nul);
            } else {
                info.push(units as u8);
                info.extend(enc(w));
            }
        }
        info.extend(num(packed.len() as u64));
        info.extend(num(block.len() as u64));
        key_blocks.extend(packed);
    }
    let info_plain_len = info.len();
    let info = if v2 {
        let mut packed = zlib(&info);
        if scrambled {
            scramble(&mut packed);
        }
        packed
    } else {
        info
    };
    let header = format!(
        "<Dictionary GeneratedByEngineVersion=\"{}\" RequiredEngineVersion=\"2.0\" Encrypted=\"{}\" Encoding=\"{}\" Format=\"Html\" Title=\"Deneme MDX\"/>\r\n\0",
        if v2 { "2.0" } else { "1.2" },
        if scrambled { 2 } else { 0 },
        if utf16 { "UTF-16" } else { "UTF-8" }
    );
    let header: Vec<u8> = header.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = (header.len() as u32).to_be_bytes().to_vec();
    out.extend(&header);
    out.extend([0; 4]);
    out.extend(num(2));
    out.extend(num(sorted.len() as u64));
    if v2 {
        out.extend(num(info_plain_len as u64));
    }
    out.extend(num(info.len() as u64));
    out.extend(num(key_blocks.len() as u64));
    if v2 {
        out.extend([0; 4]);
    }
    out.extend(&info);
    out.extend(&key_blocks);
    // Kayıt blokları: kayıt akışını ikiye böl.
    let cut = offsets[offsets.len() / 2] as usize;
    let parts = [&records[..cut], &records[cut..]];
    let packed: Vec<Vec<u8>> = parts.iter().map(|p| zlib(p)).collect();
    out.extend(num(2));
    out.extend(num(sorted.len() as u64));
    out.extend(num(2 * 2 * width as u64));
    out.extend(num(packed.iter().map(|p| p.len() as u64).sum()));
    for (p, plain) in packed.iter().zip(parts) {
        out.extend(num(p.len() as u64));
        out.extend(num(plain.len() as u64));
    }
    for p in &packed {
        out.extend(p);
    }
    let path = dir.join(format!("{name}.mdx"));
    std::fs::write(&path, out).unwrap();
    path
}

/// `mdict::descramble`'ın tersi.
fn scramble(block: &mut [u8]) {
    use ripemd::{Digest, Ripemd128};
    let mut h = Ripemd128::new();
    h.update(&block[4..8]);
    h.update(0x3695u32.to_le_bytes());
    let key = h.finalize();
    let mut prev = 0x36u8;
    for (i, b) in block[8..].iter_mut().enumerate() {
        let x = *b ^ prev ^ (i as u8) ^ key[i % key.len()];
        *b = x.rotate_left(4);
        prev = *b;
    }
}
