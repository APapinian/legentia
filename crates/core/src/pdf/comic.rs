//! Çizgi roman arşivi (CBZ): içindeki görseller ad sırasıyla (sayılar sayı
//! gibi: 2, 10'dan önce) birer sayfa olur. Açılırken bellekte her görseli bir
//! sayfaya koyan bir PDF kurulur; böylece PDF görüntüleyicinin sığdırma,
//! yakınlaştırma, kırpma ve yatayda iki sayfa özellikleri aynen çalışır.
//! JPEG'ler yeniden kodlanmadan gömülür (açılış hızlı); PNG gri tonlara
//! çevrilip sıkıştırılır.

use std::io::{Cursor, Read, Write};

use super::{MAX_FILE_BYTES, PdfError};

/// Arşiv mi: ZIP imzası.
pub(super) fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

/// Ad sıralaması için parçalar: sayılar sayı, gerisi küçük harfli metin.
fn natural_key(name: &str) -> Vec<(u8, u64, String)> {
    let mut out = Vec::new();
    let mut chars = name.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            let mut n = 0u64;
            while let Some(d) = chars.peek().and_then(|c| c.to_digit(10)) {
                n = n.saturating_mul(10).saturating_add(d as u64);
                chars.next();
            }
            out.push((0, n, String::new()));
        } else {
            let mut s = String::new();
            while let Some(&c) = chars.peek().filter(|c| !c.is_ascii_digit()) {
                s.extend(c.to_lowercase());
                chars.next();
            }
            out.push((1, 0, s));
        }
    }
    out
}

/// JPEG başlığından (SOF) genişlik, yükseklik ve renk bileşeni sayısı.
fn jpeg_info(b: &[u8]) -> Option<(u32, u32, u8)> {
    if !b.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = b[i + 1];
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        // SOF0–SOF15, DHT (C4), JPG (C8) ve DAC (CC) hariç.
        if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
            return (w > 0 && h > 0).then_some((w, h, b[i + 9]));
        }
        i += 2 + len;
    }
    None
}

/// Bir sayfanın görsel nesnesi: (genişlik, yükseklik, sözlük, akış).
fn image_object(bytes: &[u8]) -> Option<(u32, u32, String, Vec<u8>)> {
    if let Some((w, h, components)) = jpeg_info(bytes) {
        let space = match components {
            1 => "/DeviceGray",
            4 => "/DeviceCMYK",
            _ => "/DeviceRGB",
        };
        let dict = format!(
            "/Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace {space} /BitsPerComponent 8 /Filter /DCTDecode"
        );
        return Some((w, h, dict, bytes.to_vec()));
    }
    let img = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?
        .to_luma8();
    let (w, h) = img.dimensions();
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    z.write_all(img.as_raw()).ok()?;
    let dict = format!(
        "/Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode"
    );
    Some((w, h, dict, z.finish().ok()?))
}

/// CBZ baytlarından PDF.
pub(super) fn to_pdf(bytes: Vec<u8>) -> Result<Vec<u8>, PdfError> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| PdfError::Invalid(e.to_string()))?;
    let mut names: Vec<String> = archive
        .file_names()
        .flatten()
        .filter(|n| {
            let l = n.to_lowercase();
            let hidden = l.split('/').any(|p| p.starts_with('.') || p == "__macosx");
            !hidden && [".jpg", ".jpeg", ".png"].iter().any(|e| l.ends_with(e))
        })
        .map(|n| n.into_owned())
        .collect();
    names.sort_by_key(|n| natural_key(n));

    // Nesneler: 1 katalog, 2 sayfa ağacı, sonra her sayfa için sayfa, görsel, içerik.
    let mut objects: Vec<Vec<u8>> = vec![Vec::new(), Vec::new()];
    let mut kids = Vec::new();
    let mut total = 0u64;
    for name in &names {
        let Ok(entry) = archive.by_name(name) else {
            continue;
        };
        let mut data = Vec::new();
        entry
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut data)
            .map_err(PdfError::Io)?;
        total += data.len() as u64;
        if total > MAX_FILE_BYTES {
            return Err(PdfError::TooLarge(total));
        }
        let Some((w, h, dict, stream)) = image_object(&data) else {
            log::warn!("comic page not shown: {name}");
            continue;
        };
        let page = objects.len() + 1;
        kids.push(format!("{page} 0 R"));
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Contents {} 0 R /Resources << /XObject << /P {} 0 R >> >> >>",
                page + 2,
                page + 1
            )
            .into_bytes(),
        );
        let mut img = format!("<< {dict} /Length {} >>\nstream\n", stream.len()).into_bytes();
        img.extend_from_slice(&stream);
        img.extend_from_slice(b"\nendstream");
        objects.push(img);
        let content = format!("q {w} 0 0 {h} 0 0 cm /P Do Q");
        objects.push(
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            )
            .into_bytes(),
        );
    }
    if kids.is_empty() {
        return Err(PdfError::Invalid("no images in comic archive".into()));
    }
    objects[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
    objects[1] = format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        kids.len()
    )
    .into_bytes();

    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg(w: u32, h: u32, gray: u8) -> Vec<u8> {
        let img = image::GrayImage::from_pixel(w, h, image::Luma([gray]));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        out.into_inner()
    }

    #[test]
    fn pages_follow_natural_name_order_and_render() {
        let png = crate::epub::testutil::png(30, 40, 0);
        let (a, b) = (jpeg(60, 90, 255), jpeg(80, 50, 255));
        let zip = crate::epub::testutil::zip_of_bytes(&[
            ("sayfa10.jpg", a.as_slice()),
            ("sayfa2.png", png.as_slice()),
            ("__MACOSX/._sayfa2.png", b"x".as_slice()),
            ("bilgi.txt", b"x".as_slice()),
            ("Sayfa1.JPG", b.as_slice()),
        ]);
        let doc = super::super::PdfDoc::from_bytes(to_pdf(zip).unwrap()).unwrap();
        assert_eq!(doc.page_count(), 3);
        assert_eq!(doc.page_size(0), Some((80.0, 50.0)), "Sayfa1");
        assert_eq!(doc.page_size(1), Some((30.0, 40.0)), "sayfa2");
        assert_eq!(doc.page_size(2), Some((60.0, 90.0)), "sayfa10");
        // PNG sayfası siyah, JPEG beyaz çizilir.
        let dark = doc.render(1, 1.0).unwrap();
        assert!(dark.pixels.iter().all(|&p| p < 30));
        let light = doc.render(0, 1.0).unwrap();
        assert!(light.pixels.iter().all(|&p| p > 225));
        assert_eq!(jpeg_info(&a), Some((60, 90, 1)));
    }

    #[test]
    fn archives_without_pages_are_rejected() {
        let zip = crate::epub::testutil::zip_of(&[("oku.txt", "x")]);
        assert!(matches!(to_pdf(zip), Err(PdfError::Invalid(_))));
    }
}
