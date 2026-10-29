//! DjVu: saf Rust `djvu-rs` ile (deneysel; kullanıcı kararı). Belge PDF
//! görüntüleyicisinin arkasında açılır, sayfa yalnızca gerektiğinde ve
//! istenen bölgesi çizilir; böylece sığdırma, yakınlaştırma, kırpma ve
//! yatayda iki sayfa çalışır. Metin katmanı okunmaz (sayfa görüntüdür;
//! OCR kullanılabilir). Kitaplığın paniği yakalanır.

use djvu_rs::DjVuDocument;
use djvu_rs::djvu_render::{RenderOptions, RenderRect, RenderRequest};

use super::{Area, GrayImage, PdfError};

pub(super) fn parse(data: &[u8]) -> Result<DjVuDocument, PdfError> {
    std::panic::catch_unwind(|| DjVuDocument::parse(data))
        .map_err(|_| PdfError::Invalid("djvu parser panicked".into()))?
        .map_err(|e| PdfError::Invalid(format!("djvu: {e}")))
}

/// Sayfanın boyutu (punto: piksel × 72 / dpi).
pub(super) fn page_size(doc: &DjVuDocument, index: usize) -> Option<(f32, f32)> {
    let page = doc.page(index).ok()?;
    let dpi = page.dpi().max(1) as f32;
    Some((
        page.width() as f32 * 72.0 / dpi,
        page.height() as f32 * 72.0 / dpi,
    ))
}

/// Sayfanın `area` bölümü `scale` ölçeğinde, gri tonlu.
pub(super) fn render_area(
    doc: &DjVuDocument,
    index: usize,
    area: Area,
    scale: f32,
) -> Result<GrayImage, PdfError> {
    let page = doc.page(index).map_err(|_| PdfError::PageMissing(index))?;
    let (pw, ph) = page_size(doc, index).ok_or(PdfError::PageMissing(index))?;
    let full_w = (pw * scale).round().max(1.0) as u32;
    let full_h = (ph * scale).round().max(1.0) as u32;
    let x = ((area.x * scale).round().max(0.0) as u32).min(full_w - 1);
    let y = ((area.y * scale).round().max(0.0) as u32).min(full_h - 1);
    let region = RenderRect {
        x,
        y,
        width: ((area.w * scale).round().max(1.0) as u32).min(full_w - x),
        height: ((area.h * scale).round().max(1.0) as u32).min(full_h - y),
    };
    let gray = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let opts = RenderOptions::fit_to_width(page, full_w);
        RenderRequest::new(opts)
            .region(region)
            .pixmap(page)
            .map(|p| p.to_gray8())
    }))
    .map_err(|_| PdfError::Render(format!("djvu page {index} panicked")))?
    .map_err(|e| PdfError::Render(format!("djvu page {index}: {e}")))?;
    Ok(GrayImage {
        width: gray.width,
        height: gray.height,
        pixels: gray.data,
    })
}

#[cfg(test)]
mod tests {
    use super::super::PdfDoc;
    use super::*;

    /// Üst yarısı siyah, alt yarısı beyaz tek sayfalık DjVu (300 dpi, 300×400).
    fn sample() -> Vec<u8> {
        let mut bm = djvu_rs::Bitmap::new(300, 400);
        for y in 0..200 {
            for x in 0..300 {
                bm.set_black(x, y);
            }
        }
        djvu_rs::djvu_encode::PageEncoder::from_bitmap(&bm)
            .with_dpi(300)
            .encode()
            .unwrap()
    }

    #[test]
    fn djvu_pages_open_size_and_render_regions() {
        let dir = crate::storage::temp_store("djvu").dir().to_path_buf();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.djvu"), sample()).unwrap();
        let doc = PdfDoc::open(&dir.join("a.djvu")).unwrap();
        assert_eq!(doc.page_count(), 1);
        assert_eq!(doc.page_size(0), Some((72.0, 96.0)));
        let full = doc.render(0, 2.0).unwrap();
        assert_eq!((full.width, full.height), (144, 192));
        let at = |img: &GrayImage, y: u32| img.pixels[(y * img.width + img.width / 2) as usize];
        assert!(
            at(&full, 20) < 60 && at(&full, 170) > 200,
            "üst siyah, alt beyaz"
        );
        // Yalnız alt yarı: tamamı beyaz.
        let area = Area {
            x: 0.0,
            y: 60.0,
            w: 72.0,
            h: 36.0,
        };
        let bottom = doc.render_area(0, area, 2.0).unwrap();
        assert_eq!((bottom.width, bottom.height), (144, 72));
        assert!(bottom.pixels.iter().all(|&p| p > 200));
        assert!(
            doc.page_text(0).unwrap().blocks.is_empty(),
            "metin katmanı okunmaz"
        );
        assert!(
            PdfDoc::open(&{
                std::fs::write(dir.join("bozuk.djvu"), b"AT&TFORM bozuk").unwrap();
                dir.join("bozuk.djvu")
            })
            .is_err()
        );
    }
}
