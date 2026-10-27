//! PDF okuma, çizme ve metin çıkarma (hayro, saf Rust).
//!
//! Güvenlik: dosya boyutu sınırlıdır; hayro bozuk girdide paniğe kapılabildiği
//! için her sayfa çizimi panik yakalayıcıyla sarılır ve hata olarak döner.

use std::path::Path;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::Affine;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::vello_cpu::{Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit};
use hayro::{RenderCache, RenderSettings};

use crate::i18n::Msg;

pub(crate) mod text;
mod write;

pub use write::{Highlight, OcrLayer, PdfEdit};

/// Belleğe alınacak en büyük PDF (cihazda RAM sınırlı).
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
/// Çizilecek görüntünün en büyük kenarı (piksel); aşırı yakınlaştırmada bellek taşmasın.
const MAX_SIDE: f32 = 8000.0;

#[derive(Debug)]
pub enum PdfError {
    Io(std::io::Error),
    TooLarge(u64),
    Encrypted,
    Invalid(String),
    PageMissing(usize),
    /// Çizim sırasında motor hata verdi ya da paniğe kapıldı.
    Render(String),
}

impl PdfError {
    pub fn message(&self) -> Msg {
        match self {
            PdfError::Io(_) => Msg::BookFileUnreadable,
            PdfError::TooLarge(_) => Msg::BookTooLarge,
            PdfError::Encrypted => Msg::PdfEncrypted,
            PdfError::Invalid(_) | PdfError::PageMissing(_) => Msg::PdfDamaged,
            PdfError::Render(_) => Msg::PdfPageFailed,
        }
    }
}

/// Kenar kırpma için sayfanın düşük çözünürlüklü çizim ölçeği (72 dpi'nin bu katı).
const CROP_PROBE_SCALE: f32 = 0.5;
/// Bu değerden koyu pikseller içerik sayılır.
const INK: u8 = 200;
/// Kırpılan içeriğin çevresine bırakılan pay (içerik boyutunun oranı).
const CROP_PADDING: f32 = 0.02;

/// Sayfa üzerinde punto cinsinden dikdörtgen (sol üst köşe, genişlik, yükseklik).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// 8 bit gri tonlu görüntü; 0 siyah, 255 beyaz.
pub struct GrayImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub struct PdfDoc {
    pdf: Pdf,
    settings: InterpreterSettings,
}

impl PdfDoc {
    pub fn open(path: &Path) -> Result<PdfDoc, PdfError> {
        let size = std::fs::metadata(path).map_err(PdfError::Io)?.len();
        if size > MAX_FILE_BYTES {
            return Err(PdfError::TooLarge(size));
        }
        let data = std::fs::read(path).map_err(PdfError::Io)?;
        Self::from_bytes(data)
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<PdfDoc, PdfError> {
        let pdf = std::panic::catch_unwind(|| Pdf::new(data))
            .map_err(|_| PdfError::Invalid("parser panicked".into()))?
            .map_err(|e| match e {
                hayro::hayro_syntax::LoadPdfError::Decryption(_) => PdfError::Encrypted,
                other => PdfError::Invalid(format!("{other:?}")),
            })?;
        Ok(PdfDoc {
            pdf,
            settings: InterpreterSettings::default(),
        })
    }

    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// Sayfanın görüntülenen boyutu (punto, döndürme ve kırpma kutusu uygulanmış).
    pub fn page_size(&self, index: usize) -> Option<(f32, f32)> {
        self.pdf.pages().get(index).map(|p| p.render_dimensions())
    }

    /// Görüntülenen sayfa koordinatını (punto, sol üst köşe, aşağı doğru y) PDF'in
    /// kendi kullanıcı koordinatına çevirir; `/Rotate` ve kırpma kutusu hesaba katılır.
    pub fn to_user_space(&self, index: usize, x: f32, y: f32) -> Option<(f32, f32)> {
        let page = self.pdf.pages().get(index)?;
        let p = page.initial_transform(true).to_kurbo().inverse()
            * hayro::kurbo::Point::new(x as f64, y as f64);
        Some((p.x as f32, p.y as f32))
    }

    /// Sayfayı `scale` ölçeğinde (1,0 = 72 dpi) beyaz zemine gri tonlu çizer.
    pub fn render(&self, index: usize, scale: f32) -> Result<GrayImage, PdfError> {
        let (w, h) = self.page_size(index).ok_or(PdfError::PageMissing(index))?;
        self.render_area(
            index,
            Area {
                x: 0.0,
                y: 0.0,
                w,
                h,
            },
            scale,
        )
    }

    /// Sayfanın yalnızca `area` bölümünü `scale` ölçeğinde çizer; görüntü
    /// `area` × `scale` boyutundadır. Kırpılmış sayfa için tüm sayfayı büyük
    /// çizip kesmekten hızlıdır ve daha az bellek ister.
    pub fn render_area(&self, index: usize, area: Area, scale: f32) -> Result<GrayImage, PdfError> {
        let page = self
            .pdf
            .pages()
            .get(index)
            .ok_or(PdfError::PageMissing(index))?;
        let scale = scale.min(MAX_SIDE / area.w.max(area.h).max(1.0));
        let width = (area.w * scale).round().max(1.0) as u16;
        let height = (area.h * scale).round().max(1.0) as u16;
        let pixmap = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let cache = RenderCache::new();
            let mut ctx = RenderContext::new(width, height);
            let transform = Affine::translate((-(area.x * scale) as f64, -(area.y * scale) as f64))
                * Affine::scale(scale as f64)
                * page.initial_transform(true).to_kurbo();
            hayro::render_into(
                page,
                &cache,
                &self.settings,
                &RenderSettings::default(),
                &mut ctx,
                transform,
            );
            ctx.flush();
            let mut pixmap = Pixmap::new(width, height);
            let settings = RasterizerSettings {
                target_init: TargetInit::Clear(WHITE),
                ..Default::default()
            };
            ctx.render_with(&mut pixmap, &mut Resources::default(), settings);
            pixmap
        }))
        .map_err(|_| PdfError::Render(format!("page {index} panicked")))?;

        // Zemin beyaz ve opak olduğundan önceden çarpılmış RGBA doğrudan kullanılabilir.
        let pixels = pixmap
            .data_as_u8_slice()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| ((p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114) / 1000) as u8)
            .collect();
        Ok(GrayImage {
            width: width as u32,
            height: height as u32,
            pixels,
        })
    }
}

impl PdfDoc {
    /// Sayfanın beyaz kenarlar atılmış içerik alanı; boş sayfada tüm sayfa.
    pub fn content_area(&self, index: usize) -> Result<Area, PdfError> {
        let (pw, ph) = self.page_size(index).ok_or(PdfError::PageMissing(index))?;
        let probe = self.render(index, CROP_PROBE_SCALE)?;
        let full = Area {
            x: 0.0,
            y: 0.0,
            w: pw,
            h: ph,
        };
        let Some((x0, y0, x1, y1)) = ink_bounds(&probe) else {
            return Ok(full);
        };
        let sx = pw / probe.width as f32;
        let sy = ph / probe.height as f32;
        let (x, y, w, h) = (
            x0 as f32 * sx,
            y0 as f32 * sy,
            (x1 - x0 + 1) as f32 * sx,
            (y1 - y0 + 1) as f32 * sy,
        );
        let pad = CROP_PADDING * w.max(h);
        let x = (x - pad).max(0.0);
        let y = (y - pad).max(0.0);
        Ok(Area {
            x,
            y,
            w: (w + 2.0 * pad).min(pw - x),
            h: (h + 2.0 * pad).min(ph - y),
        })
    }
}

/// Koyu piksel içeren satır ve sütunların sınırı (x0, y0, x1, y1). Sayfanın en
/// dış %2'lik şeridi yok sayılır (taramalardaki kenar gölgeleri); bir satır/sütun
/// en az %0,5 oranında (en az 3) koyu piksel içeriyorsa sayılır (tek tük lekeler atlanır).
fn ink_bounds(img: &GrayImage) -> Option<(u32, u32, u32, u32)> {
    let (w, h) = (img.width, img.height);
    let (bx, by) = (w / 50, h / 50);
    let mut cols = vec![0u32; w as usize];
    let mut rows = vec![0u32; h as usize];
    for y in by..h.saturating_sub(by) {
        for x in bx..w.saturating_sub(bx) {
            if img.pixels[(y * w + x) as usize] < INK {
                cols[x as usize] += 1;
                rows[y as usize] += 1;
            }
        }
    }
    let min_col = (h / 200).max(3);
    let min_row = (w / 200).max(3);
    let x0 = cols.iter().position(|&c| c >= min_col)? as u32;
    let x1 = cols.iter().rposition(|&c| c >= min_col)? as u32;
    let y0 = rows.iter().position(|&c| c >= min_row)? as u32;
    let y1 = rows.iter().rposition(|&c| c >= min_row)? as u32;
    Some((x0, y0, x1, y1))
}

#[cfg(test)]
pub(crate) mod testutil {
    /// Tek sayfalık, standart Helvetica fontlu en küçük PDF.
    pub fn minimal_pdf(text: &str) -> Vec<u8> {
        pdf_with_pages(&[text])
    }

    /// Her metin için bir sayfa; her sayfada metin ve mavi bir dikdörtgen.
    pub fn pdf_with_pages(texts: &[&str]) -> Vec<u8> {
        let contents: Vec<String> = texts
            .iter()
            .map(|t| format!("BT /F1 24 Tf 72 720 Td ({t}) Tj ET 0 0 1 rg 72 600 200 50 re f"))
            .collect();
        pdf_with_content(&contents.iter().map(String::as_str).collect::<Vec<_>>())
    }

    /// Metin katmanı olmayan, tek gri görüntüden oluşan sayfa (taranmış sayfa taklidi).
    /// Görüntü sayfada (72, 700) noktasına `w_pt`×`h_pt` punto boyutunda yerleşir.
    pub fn pdf_with_image(image: &super::GrayImage, w_pt: f32, h_pt: f32) -> Vec<u8> {
        let content = format!("q {w_pt} 0 0 {h_pt} 72 700 cm /Im1 Do Q");
        let mut objects: Vec<Vec<u8>> = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 5 0 R /Resources << /XObject << /Im1 4 0 R >> >> >>".to_vec(),
        ];
        let mut img = format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n",
            image.width,
            image.height,
            image.pixels.len()
        )
        .into_bytes();
        img.extend_from_slice(&image.pixels);
        img.extend_from_slice(b"\nendstream");
        objects.push(img);
        objects.push(
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            )
            .into_bytes(),
        );
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
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
        out
    }

    /// Her içerik akışı için bir sayfa (Helvetica /F1 kullanılabilir).
    /// Nesne konumları (xref) hesaplanarak üretilir.
    pub fn pdf_with_content(contents: &[&str]) -> Vec<u8> {
        let n = contents.len();
        // Nesneler: 1 katalog, 2 sayfa ağacı, 3 font, sonra her sayfa için (sayfa, içerik).
        let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        for (i, content) in contents.iter().enumerate() {
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >> >>",
                5 + 2 * i
            ));
            objects.push(format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ));
        }
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
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
        out
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::minimal_pdf;
    use super::*;

    #[test]
    fn renders_a_page_at_the_requested_scale() {
        let doc = PdfDoc::from_bytes(minimal_pdf("Merhaba")).unwrap();
        assert_eq!(doc.page_count(), 1);
        assert_eq!(doc.page_size(0), Some((612.0, 792.0)));
        let img = doc.render(0, 2.0).unwrap();
        assert_eq!((img.width, img.height), (1224, 1584));
        // Mavi dikdörtgen (72,600)-(272,650) punto: koyu gri olmalı; kenar boşluğu beyaz.
        let at = |x: u32, y: u32| img.pixels[(y * img.width + x) as usize];
        assert!(at(300, (792 - 625) * 2) < 100, "dikdörtgen çizilmemiş");
        assert_eq!(at(10, 10), 255);
        // Taban çizgisi 720 punto; harf gövdesi onun üstünde (PDF'te y yukarı artar).
        let row = (792 - 726) * 2;
        assert!((150..600).any(|x| at(x, row) < 128), "metin çizilmemiş");
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(matches!(
            PdfDoc::from_bytes(b"bu bir pdf degil".to_vec()),
            Err(PdfError::Invalid(_))
        ));
        let mut truncated = minimal_pdf("x");
        truncated.truncate(truncated.len() / 2);
        let _ = PdfDoc::from_bytes(truncated).map(|d| d.render(0, 1.0));
    }

    #[test]
    fn missing_page_is_reported() {
        let doc = PdfDoc::from_bytes(minimal_pdf("x")).unwrap();
        assert!(matches!(doc.render(5, 1.0), Err(PdfError::PageMissing(5))));
    }

    #[test]
    fn ink_bounds_ignore_specks_and_page_edges() {
        let (w, h) = (300u32, 400u32);
        let mut pixels = vec![255u8; (w * h) as usize];
        let mut dark = |x: u32, y: u32| pixels[(y * w + x) as usize] = 0;
        for y in 100..200 {
            for x in 50..250 {
                dark(x, y); // içerik
            }
        }
        dark(10, 390); // tek leke
        for y in 0..h {
            dark(1, y); // tarama kenarı gölgesi (en dış şerit)
        }
        let img = GrayImage {
            width: w,
            height: h,
            pixels,
        };
        assert_eq!(ink_bounds(&img), Some((50, 100, 249, 199)));
    }

    #[test]
    fn content_area_crops_white_margins() {
        let doc = PdfDoc::from_bytes(minimal_pdf("Merhaba")).unwrap();
        let a = doc.content_area(0).unwrap();
        // İçerik: metin (72,~700-740) ve dikdörtgen (72..272, 600..650); sayfa 612x792.
        assert!(a.x > 50.0 && a.x < 72.0, "{a:?}");
        assert!(a.w < 300.0, "{a:?}");
        assert!(a.y > 30.0 && a.y + a.h < 220.0, "{a:?}");
    }

    #[test]
    fn render_area_matches_the_same_region_of_a_full_render() {
        let doc = PdfDoc::from_bytes(minimal_pdf("Merhaba")).unwrap();
        let full = doc.render(0, 2.0).unwrap();
        let area = Area {
            x: 60.0,
            y: 130.0,
            w: 240.0,
            h: 80.0,
        };
        let part = doc.render_area(0, area, 2.0).unwrap();
        assert_eq!((part.width, part.height), (480, 160));
        let mut diff = 0u64;
        for y in 0..part.height {
            for x in 0..part.width {
                let a = part.pixels[(y * part.width + x) as usize] as i64;
                let b = full.pixels[((y + 260) * full.width + x + 120) as usize] as i64;
                diff += (a - b).unsigned_abs();
            }
        }
        let mean = diff as f64 / (part.width * part.height) as f64;
        assert!(
            mean < 2.0,
            "bölge çizimi tam çizimle uyuşmuyor: ortalama fark {mean}"
        );
        assert!(part.pixels.iter().any(|&p| p < 100), "bölgede içerik yok");
    }

    #[test]
    fn huge_zoom_is_capped() {
        let doc = PdfDoc::from_bytes(minimal_pdf("x")).unwrap();
        let img = doc.render(0, 1000.0).unwrap();
        assert!(img.width.max(img.height) <= MAX_SIDE as u32 + 1);
    }
}
