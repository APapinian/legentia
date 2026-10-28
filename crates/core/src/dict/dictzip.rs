//! dictzip: parçalar hâlinde sıkıştırılmış gzip (`.dict.dz`, `.dsl.dz`).
//! gzip başlığının "RA" alanı parça boyunu ve her parçanın sıkıştırılmış
//! boyunu verir; istenen aralık yalnızca ilgili parçalar açılarak okunur.
//! "RA" alanı olmayan düz gzip dosyası tümüyle açılır (boyut sınırlı).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use flate2::{Decompress, FlushDecompress};

use super::{DictError, MAX_INDEX_BYTES};

fn invalid(what: &str) -> DictError {
    DictError::Invalid(format!("dictzip: {what}"))
}

pub(crate) enum Data {
    Plain(File),
    Chunked {
        file: File,
        chunk_len: u32,
        /// Her parçanın dosyadaki başlangıcı ve sıkıştırılmış boyu.
        chunks: Vec<(u64, u32)>,
    },
    Memory(Vec<u8>),
}

impl Data {
    /// `.dz`/`.gz` uzantılıysa sıkıştırılmış, değilse düz dosya.
    pub fn open(path: &Path) -> Result<Data, DictError> {
        let mut file = File::open(path)?;
        let compressed = path.extension().is_some_and(|x| x == "dz" || x == "gz");
        if !compressed {
            return Ok(Data::Plain(file));
        }
        let mut head = vec![0u8; 64 * 1024];
        let n = read_up_to(&mut file, &mut head)?;
        head.truncate(n);
        match parse_header(&head)? {
            Some((chunk_len, sizes, data_start)) => {
                let mut chunks = Vec::with_capacity(sizes.len());
                let mut at = data_start as u64;
                for s in sizes {
                    chunks.push((at, s as u32));
                    at += s as u64;
                }
                Ok(Data::Chunked {
                    file,
                    chunk_len,
                    chunks,
                })
            }
            None => {
                file.seek(SeekFrom::Start(0))?;
                let mut out = Vec::new();
                flate2::read::GzDecoder::new(file)
                    .take(MAX_INDEX_BYTES)
                    .read_to_end(&mut out)?;
                Ok(Data::Memory(out))
            }
        }
    }

    /// Açılmış veride `offset`'ten `len` bayt.
    pub fn read(&mut self, offset: u64, len: u32) -> Result<Vec<u8>, DictError> {
        match self {
            Data::Plain(file) => {
                file.seek(SeekFrom::Start(offset))?;
                let mut buf = vec![0u8; len as usize];
                file.read_exact(&mut buf)?;
                Ok(buf)
            }
            Data::Memory(bytes) => {
                let end = offset
                    .checked_add(len as u64)
                    .ok_or_else(|| invalid("range"))?;
                bytes
                    .get(offset as usize..end as usize)
                    .map(<[u8]>::to_vec)
                    .ok_or_else(|| invalid("range"))
            }
            Data::Chunked {
                file,
                chunk_len,
                chunks,
            } => {
                let cl = *chunk_len as u64;
                let end = offset + len as u64;
                let (first, last) = (offset / cl, end.saturating_sub(1) / cl);
                let mut out = Vec::with_capacity(len as usize);
                for c in first..=last {
                    let &(at, size) = chunks.get(c as usize).ok_or_else(|| invalid("range"))?;
                    let plain = inflate_chunk(file, at, size, *chunk_len)?;
                    let from = if c == first {
                        (offset % cl) as usize
                    } else {
                        0
                    };
                    let to = if c == last {
                        (end - c * cl) as usize
                    } else {
                        plain.len()
                    };
                    out.extend_from_slice(plain.get(from..to).ok_or_else(|| invalid("chunk"))?);
                }
                Ok(out)
            }
        }
    }
}

fn read_up_to(file: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match file.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

fn inflate_chunk(
    file: &mut File,
    at: u64,
    size: u32,
    chunk_len: u32,
) -> Result<Vec<u8>, DictError> {
    file.seek(SeekFrom::Start(at))?;
    let mut input = vec![0u8; size as usize];
    file.read_exact(&mut input)?;
    let mut out = Vec::with_capacity(chunk_len as usize);
    // Parça tam boşaltmayla (full flush) biter; akış sonu beklenmez.
    Decompress::new(false)
        .decompress_vec(&input, &mut out, FlushDecompress::Sync)
        .map_err(|e| invalid(&e.to_string()))?;
    Ok(out)
}

/// gzip başlığı: "RA" alanı varsa (parça boyu, parça boyları, verinin başı).
#[allow(clippy::type_complexity)]
fn parse_header(h: &[u8]) -> Result<Option<(u32, Vec<u16>, usize)>, DictError> {
    if h.len() < 10 || h[0] != 0x1f || h[1] != 0x8b || h[2] != 8 {
        return Err(invalid("not gzip"));
    }
    let flags = h[3];
    let mut at = 10;
    let mut ra = None;
    let u16_at = |i: usize| -> Result<u16, DictError> {
        h.get(i..i + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .ok_or_else(|| invalid("short header"))
    };
    if flags & 4 != 0 {
        let xlen = u16_at(at)? as usize;
        let mut p = at + 2;
        let end = at + 2 + xlen;
        while p + 4 <= end {
            let (si1, si2, len) = (h[p], h[p + 1], u16_at(p + 2)? as usize);
            if si1 == b'R' && si2 == b'A' {
                let (chunk_len, count) = (u16_at(p + 6)?, u16_at(p + 8)? as usize);
                let sizes = (0..count)
                    .map(|i| u16_at(p + 10 + 2 * i))
                    .collect::<Result<Vec<_>, _>>()?;
                if chunk_len == 0 {
                    return Err(invalid("chunk length 0"));
                }
                ra = Some((chunk_len as u32, sizes));
            }
            p += 4 + len;
        }
        at = end;
    }
    for flag in [8u8, 16] {
        if flags & flag != 0 {
            let nul = h[at.min(h.len())..]
                .iter()
                .position(|&b| b == 0)
                .ok_or_else(|| invalid("short header"))?;
            at += nul + 1;
        }
    }
    if flags & 2 != 0 {
        at += 2;
    }
    Ok(ra.map(|(cl, sizes)| (cl, sizes, at)))
}

#[cfg(test)]
pub(crate) fn compress(data: &[u8], chunk_len: usize) -> Vec<u8> {
    use flate2::{Compress, Compression, FlushCompress};
    let mut c = Compress::new(Compression::default(), false);
    let parts: Vec<&[u8]> = data.chunks(chunk_len).collect();
    let mut chunks = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        // Gerçek dictzip gibi: parçalar tam boşaltmayla, son parça akışı bitirerek.
        let flush = if i + 1 == parts.len() {
            FlushCompress::Finish
        } else {
            FlushCompress::Full
        };
        let mut out = Vec::with_capacity(part.len() + 64);
        let before = c.total_in();
        let mut input = *part;
        loop {
            out.reserve(1024);
            c.compress_vec(input, &mut out, flush).unwrap();
            let used = (c.total_in() - before) as usize;
            input = &part[used..];
            if input.is_empty() && out.len() < out.capacity() {
                break;
            }
        }
        chunks.push(out);
    }
    let count = chunks.len();
    let xlen = 4 + 6 + 2 * count;
    let mut out = vec![0x1f, 0x8b, 8, 4, 0, 0, 0, 0, 0, 3];
    out.extend_from_slice(&(xlen as u16).to_le_bytes());
    out.extend_from_slice(b"RA");
    out.extend_from_slice(&((6 + 2 * count) as u16).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(chunk_len as u16).to_le_bytes());
    out.extend_from_slice(&(count as u16).to_le_bytes());
    for ch in &chunks {
        out.extend_from_slice(&(ch.len() as u16).to_le_bytes());
    }
    for ch in &chunks {
        out.extend_from_slice(ch);
    }
    let mut crc = flate2::Crc::new();
    crc.update(data);
    out.extend_from_slice(&crc.sum().to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ranges_across_chunks() {
        let text: Vec<u8> = (0..20_000u32)
            .flat_map(|i| format!("{i};").into_bytes())
            .collect();
        let dir = std::env::temp_dir().join(format!("legentia-dz-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.dict.dz");
        std::fs::write(&path, compress(&text, 4096)).unwrap();
        let mut d = Data::open(&path).unwrap();
        assert!(matches!(d, Data::Chunked { .. }));
        for (off, len) in [
            (0u64, 10u32),
            (4090, 20),
            (12_000, 9000),
            (text.len() as u64 - 5, 5),
        ] {
            assert_eq!(
                d.read(off, len).unwrap(),
                &text[off as usize..(off + len as u64) as usize]
            );
        }
        assert!(
            d.read(text.len() as u64, 10).is_err(),
            "dosya dışı aralık hata"
        );
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(parse_header(b"bu gzip degil").is_err());
    }
}
