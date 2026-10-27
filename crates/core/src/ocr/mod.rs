//! Taranmış sayfalar için OCR: eğiklik düzeltme (saf Rust) ve Tesseract
//! (NixOS'ta deneysel). Sonuç paragraf bloklarıdır; yeniden akıtmaya girer.

mod deskew;
mod tesseract;

pub use deskew::{deskew, estimate_skew, rotate};
pub use tesseract::{OcrError, OcrPage, OcrWord, parse_tsv, recognize};
