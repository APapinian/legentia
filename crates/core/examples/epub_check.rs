//! Geliştirici aracı: bir klasördeki tüm EPUB'ları açar, her bölümü okur ve
//! süre/bellek/hata raporu verir. Dosyalar yalnızca okunur.
//!
//!     cargo run --release -p legentia-core --example epub_check -- <klasör>
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use legentia_core::Book;

fn main() {
    let Some(dir) = std::env::args_os().nth(1) else {
        eprintln!("usage: epub_check <dir>");
        std::process::exit(2);
    };
    let mut files = Vec::new();
    collect(Path::new(&dir), &mut files);
    files.sort();

    let mut failed = 0;
    let mut slowest = Duration::ZERO;
    for path in &files {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let started = Instant::now();
        let mut book = match Book::open(path) {
            Ok(b) => b,
            Err(e) => {
                failed += 1;
                println!("FAIL  {name}: {e:?}");
                continue;
            }
        };
        let open = started.elapsed();
        let (mut blocks, mut chars, mut bad) = (0, 0, Vec::new());
        for i in 0..book.spine.len() {
            match book.chapter(i) {
                Ok(b) => {
                    blocks += b.len();
                    chars += b.iter().map(|x| x.text.chars().count()).sum::<usize>();
                }
                Err(e) => bad.push(format!("#{i}: {e:?}")),
            }
        }
        let total = started.elapsed();
        slowest = slowest.max(total);
        if !bad.is_empty() {
            failed += 1;
        }
        println!(
            "{}  {name}\n      open {open:?}, all chapters {total:?}, spine {}, toc {}, blocks {blocks}, chars {chars}, title {:?}",
            if bad.is_empty() { "OK  " } else { "PART" },
            book.spine.len(),
            book.toc.len(),
            book.title,
        );
        for b in bad {
            println!("      chapter error {b}");
        }
    }
    println!(
        "\n{} books, {failed} with errors, slowest {slowest:?}, peak RSS {}",
        files.len(),
        peak_rss().unwrap_or_else(|| "?".into())
    );
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let hidden = path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if hidden {
            continue;
        }
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("epub"))
        {
            out.push(path);
        }
    }
}

fn peak_rss() -> Option<String> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find(|l| l.starts_with("VmHWM"))
        .map(|l| l.split_whitespace().skip(1).collect::<Vec<_>>().join(" "))
}
