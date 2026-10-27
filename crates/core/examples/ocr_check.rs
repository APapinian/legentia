//! Geliştirici aracı: OCR akışını uçtan uca ölçer. PDF sayfasını çizer, isteğe
//! bağlı olarak bilinen açıyla eğer (tarama taklidi), eğikliği düzeltir, Tesseract
//! ile okur ve sonucu sayfanın kendi metin katmanıyla karşılaştırır.
//!
//!     cargo run --release -p legentia-core --example ocr_check -- <belge.pdf> <sayfa> [eğiklik°] [dpi]
//!
//! Sayfa 1'den sayılır. Karşılaştırma için sayfanın metin katmanı olmalı.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;
use std::time::Instant;

use legentia_core::ocr;
use legentia_core::pdf::PdfDoc;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(path), Some(page)) = (
        args.first().map(PathBuf::from),
        args.get(1).and_then(|s| s.parse::<usize>().ok()),
    ) else {
        eprintln!("usage: ocr_check <file.pdf> <page> [skew degrees] [dpi]");
        std::process::exit(2);
    };
    let skew: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let dpi: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(200);
    let doc = PdfDoc::open(&path).expect("open");
    let index = page - 1;

    let truth: String = doc
        .page_blocks(index)
        .expect("text")
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    let t = Instant::now();
    let image = doc.render(index, dpi as f32 / 72.0).expect("render");
    let rendered = t.elapsed();
    let image = if skew != 0.0 {
        ocr::rotate(&image, skew)
    } else {
        image
    };

    let t = Instant::now();
    let (straight, found) = ocr::deskew(&image);
    let deskewed = t.elapsed();

    let t = Instant::now();
    let raw = ocr::recognize(&image, "tur", dpi).expect("ocr");
    let raw_time = t.elapsed();
    let fixed = ocr::recognize(&straight, "tur", dpi).expect("ocr");

    let text = |p: &ocr::OcrPage| {
        p.blocks()
            .iter()
            .map(|b| b.text.clone())
            .collect::<Vec<_>>()
            .join(" ")
    };
    println!(
        "page {page}, {}x{} px at {dpi} dpi, render {rendered:?}\n\
         skew applied {skew}°, found {found:.2}°, deskew {deskewed:?}\n\
         OCR without deskew: {raw_time:?}, character error {:.2}%\n\
         OCR with deskew:    character error {:.2}%, {} words, {} paragraphs",
        image.width,
        image.height,
        100.0 * cer(&truth, &text(&raw)),
        100.0 * cer(&truth, &text(&fixed)),
        fixed.words.len(),
        fixed.blocks().len(),
    );
}

/// Karakter hata oranı: boşluklar birleştirilmiş, tırnaklar tek biçimli metinlerde Levenshtein / uzunluk.
fn cer(truth: &str, ocr: &str) -> f64 {
    let norm = |s: &str| -> Vec<char> {
        s.replace(['’', '‘'], "'")
            .replace(['“', '”'], "\"")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .collect()
    };
    let (a, b) = (norm(truth), norm(ocr));
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, y) in b.iter().enumerate() {
            cur.push(
                (prev[j + 1] + 1)
                    .min(cur[j] + 1)
                    .min(prev[j] + usize::from(x != y)),
            );
        }
        prev = cur;
    }
    prev[b.len()] as f64 / a.len().max(1) as f64
}
