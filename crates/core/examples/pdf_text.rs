//! Geliştirici aracı: PDF sayfalarından çıkarılan paragraf ve başlıkları yazdırır.
//! Her blok bir satırdır; başlıklar `#` ile başlar. Dosya yalnızca okunur.
//!
//!     cargo run --release -p legentia-core --example pdf_text -- <belge.pdf> [ilk sayfa] [son sayfa]
//!
//! Sayfalar 1'den sayılır; verilmezse tüm belge.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

use legentia_core::epub::BlockKind;
use legentia_core::pdf::PdfDoc;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first().map(PathBuf::from) else {
        eprintln!("usage: pdf_text <file.pdf> [first page] [last page]");
        std::process::exit(2);
    };
    let doc = match PdfDoc::open(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cannot open: {e:?}");
            std::process::exit(1);
        }
    };
    let first = args
        .get(1)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let last = args
        .get(2)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(doc.page_count())
        .min(doc.page_count());
    for page in first..=last {
        match doc.page_blocks(page - 1) {
            Ok(blocks) => {
                for b in blocks {
                    let mark = match b.kind {
                        BlockKind::Heading(1) => "# ",
                        BlockKind::Heading(_) => "## ",
                        BlockKind::Paragraph => "",
                    };
                    println!("{mark}{}", b.text);
                }
            }
            Err(e) => eprintln!("page {page}: {e:?}"),
        }
    }
}
