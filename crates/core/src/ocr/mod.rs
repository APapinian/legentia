//! Taranmış sayfalar için OCR: eğiklik düzeltme (saf Rust) ve Tesseract
//! (NixOS'ta deneysel). Sonuç kelime kutuları ve paragraf bloklarıdır; yeniden
//! akıtmaya girer ve aranabilir PDF katmanı olarak yazılabilir.

mod deskew;
mod tesseract;

pub use deskew::{deskew, estimate_skew, rotate, source_point};
pub use tesseract::{OcrError, OcrPage, OcrWord, blocks_of, parse_tsv, recognize};

use crate::pdf::{OcrLayer, PdfDoc};

/// PDF sayfasını OCR'dan geçirir: `dpi` çözünürlükte çiz → eğikliği düzelt →
/// Tesseract (`lang`). Sonuç, aranabilir PDF katmanı yazmaya hazırdır.
pub fn ocr_pdf_page(doc: &PdfDoc, page: usize, lang: &str, dpi: u32) -> Result<OcrLayer, OcrError> {
    let image = doc
        .render(page, dpi as f32 / 72.0)
        .map_err(|e| OcrError::Failed(format!("{e:?}")))?;
    let (straight, angle) = deskew(&image);
    let result = recognize(&straight, lang, dpi)?;
    Ok(OcrLayer {
        page,
        dpi,
        angle,
        image_width: straight.width,
        image_height: straight.height,
        words: result.words,
    })
}
