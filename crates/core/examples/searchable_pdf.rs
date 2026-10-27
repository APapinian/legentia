//! Geliştirici aracı: metin katmanı olmayan sayfaları OCR'dan geçirip görünmez metin
//! katmanıyla yeni bir PDF yazar (uygulamadaki "Aranabilir PDF olarak kaydet" ile
//! aynı akış). Girdi değişmez.
//!
//!     cargo run --release -p legentia-core --example searchable_pdf -- <girdi.pdf> <çıktı.pdf> [dpi]
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;
use std::time::Instant;

use legentia_core::ocr;
use legentia_core::pdf::{PdfDoc, PdfEdit};

fn main() {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [input, output, rest @ ..] = args.as_slice() else {
        eprintln!("usage: searchable_pdf <in.pdf> <out.pdf> [dpi]");
        std::process::exit(2);
    };
    let dpi: u32 = rest
        .first()
        .and_then(|s| s.to_str()?.parse().ok())
        .unwrap_or(200);
    let doc = PdfDoc::open(input).expect("open");
    let mut edit = PdfEdit::open(input).expect("open for writing");
    let started = Instant::now();
    let mut added = 0;
    for page in 0..doc.page_count() {
        if !doc.page_blocks(page).unwrap_or_default().is_empty() {
            println!("page {}: already has text", page + 1);
            continue;
        }
        let t = Instant::now();
        match ocr::ocr_pdf_page(&doc, page, "tur", dpi) {
            Ok(layer) => {
                println!(
                    "page {}: skew {:.2}°, {} words, {:?}",
                    page + 1,
                    layer.angle,
                    layer.words.len(),
                    t.elapsed()
                );
                edit.add_text_layer(&layer).expect("layer");
                added += 1;
            }
            Err(e) => println!("page {}: OCR failed: {e:?}", page + 1),
        }
    }
    edit.save(output, input).expect("save");
    println!(
        "{added} pages with a text layer, {:?}, written to {}",
        started.elapsed(),
        output.display()
    );
}
