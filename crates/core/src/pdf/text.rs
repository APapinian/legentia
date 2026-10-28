//! PDF sayfasından metin çıkarma ve paragraflara ayırma (yeniden akıtma için).
//!
//! 1. Yorumlayıcıdan gelen her glifin karakteri, sayfadaki konumu, boyutu ve
//!    ilerleme genişliği toplanır (görünmez OCR katmanı dahil, döndürülmüş metin hariç).
//! 2. Glifler satırlara ayrılır; boşluk glifi yoksa geniş aralık kelime arası sayılır.
//! 3. Satırlar paragraflara birleştirilir: büyük satır aralığı, yukarı sıçrama
//!    (yeni sütun), boyut değişimi, girinti ya da kısa kalan satır yeni paragraf
//!    başlatır. Satır sonu tireleri kaldırılır, büyük yazılar başlık olur.
//!
//! Her karakterin sayfadaki kutusu blok metnindeki bayt aralığıyla birlikte
//! taşınır: sayfa görüntüsünde seçim, vurgu çizimi ve PDF açıklaması için.

use hayro::hayro_interpret::font::{Glyph, GlyphRun};
use hayro::hayro_interpret::hayro_cmap::BfString;
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, SoftMask, interpret_page,
};
use hayro::kurbo::{BezPath, Point, Rect, Vec2};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use super::{Area, PdfDoc, PdfError};
use crate::epub::{Block, BlockKind};
use crate::text::{Span, Style};

/// Glif uzayında em büyüklüğü (hayro'nun sabiti).
const UNITS_PER_EM: f64 = 1000.0;

/// Sayfa koordinatlarında (punto, sol üst köşe) tek glif.
#[derive(Debug, Clone)]
struct TextGlyph {
    text: String,
    x: f32,
    /// Taban çizgisi.
    y: f32,
    width: f32,
    size: f32,
}

#[derive(Debug, Clone)]
struct TextLine {
    text: String,
    x0: f32,
    x1: f32,
    y: f32,
    size: f32,
    boxes: Vec<CharBox>,
}

/// Bir karakterin (ya da bitişik harfin) metindeki bayt aralığı ve sayfadaki kutusu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharBox {
    pub start: usize,
    pub end: usize,
    pub area: Area,
}

/// Sayfanın metni: bloklar ve her bloğun karakter kutuları (eklenen boşluklar kutusuz).
#[derive(Debug, Clone, Default)]
pub struct PageText {
    pub blocks: Vec<Block>,
    pub boxes: Vec<Vec<CharBox>>,
}

struct Collector {
    glyphs: Vec<TextGlyph>,
}

impl<'a> Device<'a> for Collector {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _: &DrawMode) {
        for g in run.glyphs() {
            let text = match g.as_unicode() {
                Some(BfString::Char(c)) => c.to_string(),
                Some(BfString::String(s)) => s,
                None => continue,
            };
            let t = props.transform * g.transform();
            let origin = t * Point::ZERO;
            let along: Vec2 = t * Point::new(UNITS_PER_EM, 0.0) - origin;
            let up: Vec2 = t * Point::new(0.0, UNITS_PER_EM) - origin;
            // Yatay olmayan (döndürülmüş, dikey) metin akışa katılmaz.
            if along.x <= 0.0 || along.y.abs() > 0.1 * along.x {
                continue;
            }
            let size = up.hypot() as f32;
            let advance = match &**g {
                Glyph::Outline(o) => o
                    .advance_width()
                    .map(|w| (w as f64 / UNITS_PER_EM) * along.x),
                Glyph::Type3(_) => None,
            };
            self.glyphs.push(TextGlyph {
                text,
                x: origin.x as f32,
                y: origin.y as f32,
                width: advance.map_or(size * 0.5, |a| a as f32),
                size,
            });
        }
    }
}

impl PdfDoc {
    /// Sayfanın metni, paragraf ve başlık blokları olarak. Metin katmanı yoksa boş.
    pub fn page_blocks(&self, index: usize) -> Result<Vec<Block>, PdfError> {
        Ok(self.page_text(index)?.blocks)
    }

    /// Sayfanın metni ve karakter kutuları.
    pub fn page_text(&self, index: usize) -> Result<PageText, PdfError> {
        let glyphs = self.page_glyphs(index)?;
        Ok(paragraphs(&lines(&glyphs)))
    }

    fn page_glyphs(&self, index: usize) -> Result<Vec<TextGlyph>, PdfError> {
        let page = self
            .pdf
            .pages()
            .get(index)
            .ok_or(PdfError::PageMissing(index))?;
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (w, h) = page.render_dimensions();
            let cache = InterpreterCache::new();
            let mut ctx = Context::new(
                page.initial_transform(true).to_kurbo(),
                Rect::new(0.0, 0.0, w as f64, h as f64),
                &cache,
                page.xref(),
                self.settings.clone(),
            );
            let mut device = Collector { glyphs: Vec::new() };
            interpret_page(page, &mut ctx, &mut device);
            device.glyphs
        }))
        .map_err(|_| PdfError::Render(format!("text of page {index} panicked")))
    }
}

/// Glifleri içerik sırasıyla satırlara ayırır.
fn lines(glyphs: &[TextGlyph]) -> Vec<TextLine> {
    let mut out: Vec<TextLine> = Vec::new();
    let mut end = 0.0f32; // son glifin sağ kenarı
    for g in glyphs {
        let same_line = out.last().is_some_and(|l| {
            let size = l.size.max(g.size);
            (g.y - l.y).abs() < 0.5 * size && g.x > end - 1.5 * size
        });
        if !same_line {
            out.push(TextLine {
                text: String::new(),
                x0: g.x,
                x1: g.x,
                y: g.y,
                size: g.size,
                boxes: Vec::new(),
            });
        }
        let line = out.last_mut().expect("az önce eklendi");
        let gap = g.x - end;
        let is_space = g.text.chars().all(char::is_whitespace);
        if same_line && gap > 0.2 * g.size && !is_space && !line.text.ends_with(' ') {
            line.text.push(' ');
        }
        if is_space {
            if !line.text.is_empty() && !line.text.ends_with(' ') {
                line.text.push(' ');
            }
        } else {
            push_glyph(line, g);
            line.size = line.size.max(g.size);
        }
        end = g.x + g.width;
        line.x1 = line.x1.max(end);
    }
    for l in &mut out {
        let len = l.text.trim_end().len();
        l.text.truncate(len);
    }
    out.retain(|l| !l.text.is_empty());
    out
}

/// Glifin metnini NFC olarak satıra ekler. Birleştiren işaret (ör. `I` + U+0307)
/// önceki karakterle birleştirilir, kutusu da onunkine katılır; böylece bayt
/// aralıkları normalleştirmeden sonra da doğru kalır.
fn push_glyph(line: &mut TextLine, g: &TextGlyph) {
    let area = Area {
        x: g.x,
        y: g.y - 0.8 * g.size,
        w: g.width,
        h: g.size,
    };
    let combining = g.text.chars().next().is_some_and(is_combining_mark);
    if combining && let Some(last) = line.boxes.last_mut() {
        let joined: String = line.text[last.start..]
            .chars()
            .chain(g.text.chars())
            .nfc()
            .collect();
        line.text.truncate(last.start);
        line.text.push_str(&joined);
        last.end = line.text.len();
        let right = (last.area.x + last.area.w).max(area.x + area.w);
        last.area.w = right - last.area.x;
        return;
    }
    let start = line.text.len();
    line.text.extend(g.text.nfc());
    line.boxes.push(CharBox {
        start,
        end: line.text.len(),
        area,
    });
}

/// Satırları paragraflara ve başlıklara birleştirir.
fn paragraphs(lines: &[TextLine]) -> PageText {
    let mut out = PageText::default();
    if lines.is_empty() {
        return out;
    }
    let body = body_size(lines);
    let spacing = line_spacing(lines, body);

    let mut text = String::new();
    let mut boxes: Vec<CharBox> = Vec::new();
    let mut para_size = 0.0f32;
    for (i, line) in lines.iter().enumerate() {
        let new_para = match i.checked_sub(1).map(|p| (p, &lines[p])) {
            None => true,
            Some((p, prev)) => {
                let dy = line.y - prev.y;
                let size_changed = (line.size - prev.size).abs() > 0.15 * prev.size.max(line.size);
                // Tireyle biten satır kelime ortasında bölünmüştür, devam eder.
                let short_prev = prev.x1 < column_right(lines, p) - 3.0 * prev.size
                    && !ends_with_hyphen(&prev.text);
                let indented = line.x0 > prev.x0 + 1.2 * line.size;
                dy < 0.0 || dy > 1.6 * spacing || size_changed || short_prev || indented
            }
        };
        if new_para && !text.is_empty() {
            out.blocks
                .push(block(std::mem::take(&mut text), para_size, body));
            out.boxes.push(std::mem::take(&mut boxes));
        }
        if new_para {
            para_size = line.size;
        }
        let before = text.len();
        join_line(&mut text, &line.text);
        let base = text.len() - line.text.len();
        // Satır sonu tiresi silindiyse kutusu da gider.
        boxes.retain(|b| b.end <= base.min(before));
        boxes.extend(line.boxes.iter().map(|b| CharBox {
            start: b.start + base,
            end: b.end + base,
            area: b.area,
        }));
    }
    if !text.is_empty() {
        out.blocks.push(block(text, para_size, body));
        out.boxes.push(boxes);
    }
    out
}

/// Satırın bulunduğu sütunun sağ kenarı: içerik sırasında ondan sonraki dört
/// satırdan yatayda çakışanların en sağ ucu (aynı sütunun devamı); sonrası yoksa
/// öncekiler. Sayfa başındaki tam genişlik satırları sütun kenarını bozmaz.
fn column_right(lines: &[TextLine], index: usize) -> f32 {
    const NEIGHBOURS: usize = 4;
    let line = &lines[index];
    let overlapping =
        |l: &&TextLine| l.x0 < line.x1 && l.x1 > line.x0 && (l.y - line.y).abs() < 6.0 * line.size;
    let after = &lines[index + 1..(index + 1 + NEIGHBOURS).min(lines.len())];
    let before = &lines[index.saturating_sub(NEIGHBOURS)..index];
    let pick = if after.iter().any(|l| overlapping(&l)) {
        after
    } else {
        before
    };
    pick.iter()
        .filter(overlapping)
        .map(|l| l.x1)
        .fold(line.x1, f32::max)
}

fn ends_with_hyphen(text: &str) -> bool {
    let mut chars = text.chars().rev();
    chars.next() == Some('-') && chars.next().is_some_and(char::is_alphabetic)
}

/// Satırı paragrafa ekler; satır sonu tiresi ve küçük harfle devam eden kelime birleştirilir.
pub(crate) fn join_line(text: &mut String, line: &str) {
    if text.is_empty() {
        text.push_str(line);
        return;
    }
    if ends_with_hyphen(text) && line.chars().next().is_some_and(char::is_lowercase) {
        text.pop();
    } else {
        text.push(' ');
    }
    text.push_str(line);
}

fn block(text: String, size: f32, body: f32) -> Block {
    let kind = if size > 1.6 * body {
        BlockKind::Heading(1)
    } else if size > 1.25 * body {
        BlockKind::Heading(2)
    } else {
        BlockKind::Paragraph
    };
    let spans = vec![Span {
        start: 0,
        end: text.len(),
        style: Style::default(),
    }];
    Block {
        kind,
        text,
        spans,
        ..Block::default()
    }
}

/// Karakter sayısına göre en yaygın yazı boyutu (gövde metni).
fn body_size(lines: &[TextLine]) -> f32 {
    let mut sizes: Vec<(f32, usize)> = Vec::new();
    for l in lines {
        let n = l.text.chars().count();
        match sizes.iter_mut().find(|(s, _)| (s - l.size).abs() < 0.5) {
            Some((_, count)) => *count += n,
            None => sizes.push((l.size, n)),
        }
    }
    sizes
        .iter()
        .max_by_key(|(_, n)| *n)
        .map_or(10.0, |(s, _)| *s)
}

/// Gövde metni satırları arasındaki tipik dikey aralık. Yalnızca aynı boyuttaki
/// ardışık satırlar sayılır (başlık→gövde geçişi hariç) ve alt dilim (%30)
/// alınır: paragraf boşlukları satır aralığından büyüktür, sayıca azdır.
fn line_spacing(lines: &[TextLine], body: f32) -> f32 {
    let mut gaps: Vec<f32> = lines
        .windows(2)
        .filter(|w| (w[0].size - w[1].size).abs() <= 0.15 * w[0].size.max(w[1].size))
        .map(|w| w[1].y - w[0].y)
        .filter(|&d| d > 0.5 * body && d < 3.0 * body)
        .collect();
    gaps.sort_by(f32::total_cmp);
    gaps.get(gaps.len() * 3 / 10).copied().unwrap_or(1.2 * body)
}

#[cfg(test)]
mod tests {
    use super::super::testutil::pdf_with_content;
    use super::*;

    fn texts(blocks: &[Block]) -> Vec<(BlockKind, &str)> {
        blocks.iter().map(|b| (b.kind, b.text.as_str())).collect()
    }

    #[test]
    fn joins_lines_into_paragraphs_and_removes_line_end_hyphens() {
        // Başlık (24 pt, gövdenin 2 katı → 1. düzey), iki satırlık paragraf (satır sonu tireli), boşluk, ikinci paragraf.
        let content = "BT /F1 24 Tf 72 720 Td (Birinci Bolum) Tj ET \
            BT /F1 12 Tf 72 690 Td (Bu satir kardes-) Tj 0 -14 Td (lerimiz ile devam eder ve biter.) Tj ET \
            BT /F1 12 Tf 72 640 Td (Ikinci paragraf burada.) Tj ET";
        let doc = PdfDoc::from_bytes(pdf_with_content(&[content])).unwrap();
        let blocks = doc.page_blocks(0).unwrap();
        assert_eq!(
            texts(&blocks),
            vec![
                (BlockKind::Heading(1), "Birinci Bolum"),
                (
                    BlockKind::Paragraph,
                    "Bu satir kardeslerimiz ile devam eder ve biter."
                ),
                (BlockKind::Paragraph, "Ikinci paragraf burada."),
            ]
        );
    }

    #[test]
    fn char_boxes_follow_the_joined_paragraph_text() {
        let content = "BT /F1 12 Tf 72 690 Td (Bu satir kardes-) Tj 0 -14 Td (lerimiz ile.) Tj ET";
        let doc = PdfDoc::from_bytes(pdf_with_content(&[content])).unwrap();
        let page = doc.page_text(0).unwrap();
        let (text, boxes) = (&page.blocks[0].text, &page.boxes[0]);
        assert_eq!(text, "Bu satir kardeslerimiz ile.");
        // Her kutu boşluk olmayan bir karaktere düşer; tire kutusu atılmıştır.
        assert_eq!(boxes.len(), text.chars().filter(|c| *c != ' ').count());
        let at = |s: &str| {
            boxes
                .iter()
                .find(|b| &text[b.start..b.end] == s)
                .unwrap()
                .area
        };
        // "l" ikinci satırda (taban 792-676=116 punto aşağıda), "B" birincide.
        let (b, l) = (at("B"), at("l"));
        assert!(
            (b.x - 72.0).abs() < 0.5 && (l.x - 72.0).abs() < 0.5,
            "{b:?} {l:?}"
        );
        assert!((l.y - b.y - 14.0).abs() < 0.5, "{b:?} {l:?}");
        assert!(
            b.y < 792.0 - 690.0 && b.y + b.h > 792.0 - 690.0,
            "taban çizgisi kutunun içinde"
        );
    }

    #[test]
    fn a_combining_dot_is_merged_into_its_letter() {
        let mut line = TextLine {
            text: String::new(),
            x0: 0.0,
            x1: 0.0,
            y: 10.0,
            size: 10.0,
            boxes: Vec::new(),
        };
        let glyph = |text: &str, x: f32| TextGlyph {
            text: text.into(),
            x,
            y: 10.0,
            width: 5.0,
            size: 10.0,
        };
        for g in [glyph("I", 0.0), glyph("\u{307}", 2.0), glyph("s", 5.0)] {
            push_glyph(&mut line, &g);
        }
        assert_eq!(line.text, "İs");
        assert_eq!(
            line.boxes
                .iter()
                .map(|b| (b.start, b.end))
                .collect::<Vec<_>>(),
            vec![(0, 2), (2, 3)]
        );
    }

    #[test]
    fn separately_positioned_words_get_spaces() {
        // Kelimeler ayrı ayrı konumlanmış, boşluk karakteri yok (TJ aralıkları).
        let content = "BT /F1 12 Tf 72 700 Td [(iki) -400 (kelime)] TJ ET";
        let doc = PdfDoc::from_bytes(pdf_with_content(&[content])).unwrap();
        assert_eq!(
            texts(&doc.page_blocks(0).unwrap()),
            vec![(BlockKind::Paragraph, "iki kelime")]
        );
    }

    #[test]
    fn kerning_does_not_split_words() {
        let content = "BT /F1 12 Tf 72 700 Td [(ke) -20 (lime)] TJ ET";
        let doc = PdfDoc::from_bytes(pdf_with_content(&[content])).unwrap();
        assert_eq!(
            texts(&doc.page_blocks(0).unwrap()),
            vec![(BlockKind::Paragraph, "kelime")]
        );
    }

    #[test]
    fn short_centered_heading_line_ends_its_paragraph() {
        let content = "BT /F1 12 Tf 250 720 Td (ORTALI) Tj ET \
            BT /F1 12 Tf 72 706 Td (Uzun bir govde satiri burada sayfa genisligince uzanir ve biter.) Tj ET";
        let doc = PdfDoc::from_bytes(pdf_with_content(&[content])).unwrap();
        assert_eq!(doc.page_blocks(0).unwrap().len(), 2);
    }

    #[test]
    fn second_column_starts_a_new_paragraph() {
        let content = "BT /F1 12 Tf 72 700 Td (sol sutun sonu) Tj ET BT /F1 12 Tf 320 760 Td (sag sutun basi) Tj ET";
        let doc = PdfDoc::from_bytes(pdf_with_content(&[content])).unwrap();
        assert_eq!(doc.page_blocks(0).unwrap().len(), 2);
    }

    #[test]
    fn page_without_text_has_no_blocks() {
        let doc = PdfDoc::from_bytes(pdf_with_content(&["0 0 1 rg 72 600 200 50 re f"])).unwrap();
        assert!(doc.page_blocks(0).unwrap().is_empty());
    }

    #[test]
    fn rotated_text_is_skipped() {
        let content =
            "BT /F1 12 Tf 0 1 -1 0 300 300 Tm (dikey) Tj ET BT /F1 12 Tf 72 700 Td (yatay) Tj ET";
        let doc = PdfDoc::from_bytes(pdf_with_content(&[content])).unwrap();
        assert_eq!(
            texts(&doc.page_blocks(0).unwrap()),
            vec![(BlockKind::Paragraph, "yatay")]
        );
    }
}
