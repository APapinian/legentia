//! Tesseract komut satırı aracıyla OCR (yalnızca masaüstü/NixOS deneyi).
//!
//! Çıktı TSV'dir: her kelime için kutu, güvenilirlik ve blok/paragraf/satır
//! numarası. Cihazda bu araç bulunmayacak; orada kullanılacak motor M10'da seçilecek.

use std::path::PathBuf;
use std::process::Command;

use crate::epub::{Block, BlockKind};
use crate::pdf::GrayImage;
use crate::pdf::text::join_line;
use crate::text::{Span, Style};

#[derive(Debug)]
pub enum OcrError {
    /// Tesseract bulunamadı ya da çalıştırılamadı.
    NotInstalled(String),
    Failed(String),
}

/// Tanınan tek kelime; kutu, OCR'a verilen görüntünün pikselleriyle.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OcrWord {
    pub text: String,
    /// 0–100; Tesseract'ın güveni.
    pub confidence: f32,
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    pub block: u32,
    pub paragraph: u32,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OcrPage {
    pub width: u32,
    pub height: u32,
    pub words: Vec<OcrWord>,
}

/// Tesseract'ın yolu: `LEGENTIA_TESSERACT` (nix develop ayarlar), yoksa PATH'teki `tesseract`.
fn binary() -> PathBuf {
    std::env::var_os("LEGENTIA_TESSERACT").map_or_else(|| PathBuf::from("tesseract"), PathBuf::from)
}

/// Görüntüyü `lang` (ör. "tur") diliyle okur. `dpi`, görüntünün çözünürlüğü.
pub fn recognize(img: &GrayImage, lang: &str, dpi: u32) -> Result<OcrPage, OcrError> {
    recognize_with(&binary(), img, lang, dpi)
}

fn recognize_with(
    binary: &std::path::Path,
    img: &GrayImage,
    lang: &str,
    dpi: u32,
) -> Result<OcrPage, OcrError> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let file = std::env::temp_dir().join(format!(
        "legentia-ocr-{}-{}.pgm",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let mut pgm = format!("P5\n{} {}\n255\n", img.width, img.height).into_bytes();
    pgm.extend_from_slice(&img.pixels);
    std::fs::write(&file, pgm).map_err(|e| OcrError::Failed(e.to_string()))?;

    let output = Command::new(binary)
        .arg(&file)
        .args([
            "stdout",
            "-l",
            lang,
            "--psm",
            "3",
            "--dpi",
            &dpi.to_string(),
            "tsv",
        ])
        .output();
    let _ = std::fs::remove_file(&file);
    let output = output.map_err(|e| OcrError::NotInstalled(e.to_string()))?;
    if !output.status.success() {
        return Err(OcrError::Failed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    let tsv = String::from_utf8_lossy(&output.stdout);
    Ok(OcrPage {
        width: img.width,
        height: img.height,
        words: parse_tsv(&tsv),
    })
}

/// TSV'nin kelime satırları (düzey 5). Sütunlar: level page block par line word
/// left top width height conf text.
pub fn parse_tsv(tsv: &str) -> Vec<OcrWord> {
    let mut out = Vec::new();
    for row in tsv.lines().skip(1) {
        let cols: Vec<&str> = row.splitn(12, '\t').collect();
        if cols.len() < 12 || cols[0] != "5" {
            continue;
        }
        let num = |i: usize| cols[i].trim().parse::<u32>().unwrap_or(0);
        let text = cols[11].trim();
        if text.is_empty() {
            continue;
        }
        out.push(OcrWord {
            text: text.to_string(),
            confidence: cols[10].trim().parse().unwrap_or(0.0),
            left: num(6),
            top: num(7),
            width: num(8),
            height: num(9),
            block: num(2),
            paragraph: num(3),
            line: num(4),
        });
    }
    out
}

impl OcrPage {
    /// Tanınan metin, paragraf ve başlık blokları olarak; bkz. [`blocks_of`].
    pub fn blocks(&self) -> Vec<Block> {
        blocks_of(&self.words)
    }
}

/// Kelimelerden paragraf ve başlık blokları (Tesseract'ın paragraf sınırlarıyla).
/// Satır sonu tireleri birleştirilir; ortalamadan belirgin büyük harf boyu başlık sayılır.
pub fn blocks_of(words: &[OcrWord]) -> Vec<Block> {
    {
        let mut heights: Vec<u32> = words.iter().map(|w| w.height).collect();
        heights.sort_unstable();
        let body = heights.get(heights.len() / 2).copied().unwrap_or(1).max(1) as f32;

        let mut out = Vec::new();
        let mut i = 0;
        while i < words.len() {
            let key = (words[i].block, words[i].paragraph);
            let mut j = i;
            let mut text = String::new();
            let mut para_heights = Vec::new();
            while j < words.len() && (words[j].block, words[j].paragraph) == key {
                let line = words[j].line;
                let mut line_text = String::new();
                while j < words.len()
                    && (words[j].block, words[j].paragraph) == key
                    && words[j].line == line
                {
                    if !line_text.is_empty() {
                        line_text.push(' ');
                    }
                    line_text.push_str(&words[j].text);
                    para_heights.push(words[j].height);
                    j += 1;
                }
                join_line(&mut text, &line_text);
            }
            para_heights.sort_unstable();
            let h = para_heights
                .get(para_heights.len() / 2)
                .copied()
                .unwrap_or(0) as f32;
            let kind = if h > 1.8 * body {
                BlockKind::Heading(1)
            } else if h > 1.35 * body {
                BlockKind::Heading(2)
            } else {
                BlockKind::Paragraph
            };
            let spans = vec![Span {
                start: 0,
                end: text.len(),
                style: Style::default(),
            }];
            out.push(Block {
                kind,
                text,
                spans,
                ..Block::default()
            });
            i = j;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TSV: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext
1\t1\t0\t0\t0\t0\t0\t0\t1000\t1400\t-1\t
5\t1\t1\t1\t1\t1\t100\t100\t300\t60\t96.5\tBAŞLIK
5\t1\t2\t1\t1\t1\t100\t200\t80\t30\t95.0\tAğaçların
5\t1\t2\t1\t1\t2\t190\t200\t120\t30\t93.1\tgölgesinde
5\t1\t2\t1\t1\t3\t320\t200\t60\t30\t90.0\tkar-
5\t1\t2\t1\t2\t1\t100\t240\t90\t30\t92.0\tdeşlerimiz
5\t1\t2\t1\t2\t2\t200\t240\t60\t30\t91.0\toturdu.
5\t1\t2\t2\t1\t1\t100\t320\t60\t30\t88.0\tYeni
5\t1\t2\t2\t1\t2\t170\t320\t90\t30\t87.0\tparagraf.
";

    #[test]
    fn parses_word_rows_only() {
        let words = parse_tsv(TSV);
        assert_eq!(words.len(), 8);
        assert_eq!(words[1].text, "Ağaçların");
        assert_eq!(
            (words[1].left, words[1].top, words[1].width, words[1].height),
            (100, 200, 80, 30)
        );
        assert_eq!(
            (words[3].block, words[3].paragraph, words[3].line),
            (2, 1, 1)
        );
        assert!((words[0].confidence - 96.5).abs() < 0.01);
    }

    #[test]
    fn groups_paragraphs_joins_hyphens_and_finds_headings() {
        let page = OcrPage {
            width: 1000,
            height: 1400,
            words: parse_tsv(TSV),
        };
        let blocks: Vec<_> = page
            .blocks()
            .into_iter()
            .map(|b| (b.kind, b.text))
            .collect();
        assert_eq!(
            blocks,
            vec![
                (BlockKind::Heading(1), "BAŞLIK".to_string()),
                (
                    BlockKind::Paragraph,
                    "Ağaçların gölgesinde kardeşlerimiz oturdu.".to_string()
                ),
                (BlockKind::Paragraph, "Yeni paragraf.".to_string()),
            ]
        );
    }

    #[test]
    fn missing_binary_is_reported() {
        let img = GrayImage {
            width: 10,
            height: 10,
            pixels: vec![255; 100],
        };
        let result = recognize_with(std::path::Path::new("/olmayan/tesseract"), &img, "tur", 300);
        assert!(matches!(result, Err(OcrError::NotInstalled(_))));
    }
}
