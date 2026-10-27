//! Geliştirici aracı: bir klasördeki (ya da tek bir) PDF'in tüm sayfalarını
//! Libra 2 genişliğine (1264 px) sığdırarak çizer; süre, bellek ve hata raporu verir.
//! İsteğe bağlı çıktı klasörüne her belgenin ilk sayfasını PGM olarak yazar.
//! `PDF_CHECK_MEM=1` ile sayfa başına en yüksek bellek ölçülür ve en çok
//! bellek isteyen sayfalar listelenir (Linux `/proc/self/clear_refs`).
//! Dosyalar yalnızca okunur.
//!
//!     cargo run --release -p legentia-core --example pdf_check -- <klasör | belge.pdf> [çıktı klasörü]
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use legentia_core::LIBRA2_WIDTH;
use legentia_core::pdf::PdfDoc;

fn main() {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let Some(input) = args.first() else {
        eprintln!("usage: pdf_check <dir | file.pdf> [out dir]");
        std::process::exit(2);
    };
    let out = args.get(1);
    let mut files = Vec::new();
    if input.is_dir() {
        collect(input, &mut files);
    } else {
        files.push(input.clone());
    }
    files.sort();

    let (mut pages_total, mut failed_pages, mut slowest) = (0, 0, Duration::ZERO);
    for (n, path) in files.iter().enumerate() {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let started = Instant::now();
        let doc = match PdfDoc::open(path) {
            Ok(d) => d,
            Err(e) => {
                println!("FAIL  {name}: {e:?}");
                continue;
            }
        };
        let open = started.elapsed();
        let mut times = Vec::new();
        let per_page_mem = std::env::var_os("PDF_CHECK_MEM").is_some();
        let mut mem: Vec<(u64, usize)> = Vec::new();
        for i in 0..doc.page_count() {
            let (w, _) = doc.page_size(i).unwrap_or((595.0, 842.0));
            if per_page_mem {
                // En yüksek bellek sayacını sıfırla: ölçüm yalnızca bu sayfayı kapsar.
                let _ = std::fs::write("/proc/self/clear_refs", "5");
            }
            let t = Instant::now();
            match doc.render(i, LIBRA2_WIDTH as f32 / w) {
                Ok(img) => {
                    if i == 0
                        && let Some(dir) = out
                    {
                        let mut pgm =
                            format!("P5\n{} {}\n255\n", img.width, img.height).into_bytes();
                        pgm.extend_from_slice(&img.pixels);
                        let _ = std::fs::create_dir_all(dir);
                        let _ = std::fs::write(dir.join(format!("{n}.pgm")), pgm);
                    }
                }
                Err(e) => {
                    failed_pages += 1;
                    println!("      page {i}: {e:?}");
                }
            }
            times.push(t.elapsed());
            if per_page_mem {
                mem.push((peak_kb().unwrap_or(0), i));
            }
        }
        if per_page_mem {
            mem.sort_unstable_by(|a, b| b.cmp(a));
            let top: Vec<String> = mem
                .iter()
                .take(5)
                .map(|(kb, i)| format!("p{i}={} MB", kb / 1024))
                .collect();
            let median = mem.get(mem.len() / 2).map_or(0, |m| m.0 / 1024);
            println!(
                "      memory per page: median {median} MB, highest {}",
                top.join(", ")
            );
        }
        pages_total += times.len();
        times.sort();
        let median = times.get(times.len() / 2).copied().unwrap_or_default();
        let max = times.last().copied().unwrap_or_default();
        slowest = slowest.max(max);
        println!(
            "OK    [{n}] {name}\n      open {open:?}, {} pages, median {median:?}, slowest {max:?}",
            doc.page_count()
        );
    }
    println!(
        "\n{} files, {pages_total} pages, {failed_pages} failed pages, slowest page {slowest:?}, peak RSS {}",
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
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
        {
            out.push(path);
        }
    }
}

fn peak_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

fn peak_rss() -> Option<String> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM"))?;
    Some(
        line.split_whitespace()
            .skip(1)
            .collect::<Vec<_>>()
            .join(" "),
    )
}
