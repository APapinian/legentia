//! Açık kitap: okunan bölüm, sayfaları, kayıtlı konum ve yer imleri.
//!
//! PDF'te her PDF sayfası bir "bölüm"dür (`spine` = PDF sayfa sırası). Sayfa
//! görünümünde bölüm tek ekrandır; yeniden akıtmada sayfanın metni EPUB gibi
//! ekranlara dizilir. İki görünümde de konum aynı anlamı taşır.

use serde::{Deserialize, Serialize};

use super::settings::Settings;
use super::ui::{BOTTOM, TOP};
use crate::epub::{Block, BlockKind, Book, TocEntry};
use crate::i18n::{Msg, tr};
use crate::ocr;
use crate::pdf::{OcrLayer, PdfDoc};
use crate::storage::Store;
use crate::text::{Align, Family, Line, ParaStyle, Style, Text, layout};

/// Bir başlığın altında en az bu kadar satır yer yoksa başlık sonraki sayfaya geçer.
const LINES_AFTER_HEADING: f32 = 3.0;
/// OCR için sayfanın çizildiği çözünürlük (deneyde 150 dpi yeterliydi; pay bırakıldı).
const OCR_DPI: u32 = 200;
/// Yer imi listesinde gösterilen alıntının en fazla karakter sayısı.
const SNIPPET_CHARS: usize = 70;

/// Bölüm içinde kararlı konum: blok sırası ve blok metnindeki bayt konumu.
/// Sayfa numarasından farklı olarak yazı ayarı değişince de geçerlidir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct Position {
    pub block: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    pub spine: usize,
    pub position: Position,
    /// Sayfanın ilk kelimeleri; listede yer imini tanıtmak için.
    pub snippet: String,
}

/// Kitap başına kalıcı durum (`books/<kimlik>.json`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BookState {
    /// Kitap en az bir kez okunduysa son okunan yer.
    pub last: Option<(usize, Position)>,
    /// Kitabın yaklaşık ne kadarının okunduğu (yüzde).
    pub percent: u32,
    pub bookmarks: Vec<Bookmark>,
}

pub(super) struct PlacedLine {
    pub line: Line,
    pub baseline: i32,
}

pub(super) struct Page {
    pub lines: Vec<PlacedLine>,
    pub start: Position,
}

pub(super) enum Target {
    Start,
    End,
    At(Position),
}

pub(super) enum Doc {
    Epub(Book),
    Pdf(PdfDoc),
}

pub(super) struct Session {
    pub doc: Doc,
    /// Kitabın başlığı yoksa gösterilecek ad (dosya adı).
    pub name: String,
    /// Kitap dosyasının yolu.
    pub path: std::path::PathBuf,
    /// OCR sonuçlarının kitap başına önbelleği (`ocr/<kimlik>/<sayfa>.json`).
    pub ocr_cache: Option<Store>,
    pub id: String,
    pub state: BookState,
    pub spine: usize,
    pub blocks: Vec<Block>,
    pub pages: Vec<Page>,
    pub page: usize,
}

impl Session {
    pub fn new(doc: Doc, name: String, id: String, state: BookState) -> Self {
        Self {
            doc,
            name,
            path: std::path::PathBuf::new(),
            ocr_cache: None,
            id,
            state,
            spine: 0,
            blocks: Vec::new(),
            pages: Vec::new(),
            page: 0,
        }
    }

    pub fn chapter_count(&self) -> usize {
        match &self.doc {
            Doc::Epub(book) => book.spine.len(),
            Doc::Pdf(pdf) => pdf.page_count(),
        }
    }

    /// Şu an PDF sayfa görüntüsü mü gösteriliyor (yeniden akıtma kapalı ya da sayfada metin yok)?
    pub fn showing_pdf_image(&self) -> bool {
        self.pdf().is_some() && self.blocks.is_empty()
    }

    pub fn pdf(&self) -> Option<&PdfDoc> {
        match &self.doc {
            Doc::Pdf(pdf) => Some(pdf),
            Doc::Epub(_) => None,
        }
    }

    /// Kitabın başlığı, yoksa dosya adı.
    pub fn title(&self) -> String {
        match &self.doc {
            Doc::Epub(book) => book.title.clone().unwrap_or_else(|| self.name.clone()),
            Doc::Pdf(_) => self.name.clone(),
        }
    }

    /// Kitabın kendi içindekiler listesi (PDF'te şimdilik yok).
    pub fn toc(&self) -> &[TocEntry] {
        match &self.doc {
            Doc::Epub(book) => &book.toc,
            Doc::Pdf(_) => &[],
        }
    }

    fn language(&self) -> Option<&str> {
        match &self.doc {
            Doc::Epub(book) => book.language.as_deref(),
            Doc::Pdf(_) => None,
        }
    }

    /// Bölümü okuyup sayfalara dizer. Hata olursa oturum değişmez.
    pub fn load_chapter(
        &mut self,
        text: &Text,
        settings: &Settings,
        width: u32,
        spine: usize,
        target: Target,
    ) -> Result<(), Msg> {
        let blocks = match &mut self.doc {
            Doc::Epub(book) => book.chapter(spine).map_err(|e| {
                log::error!("cannot read chapter {spine}: {e:?}");
                e.message()
            })?,
            // Metin çıkarılamazsa sayfa görüntüsüne düşülür; okuma kesilmez.
            Doc::Pdf(pdf) if settings.pdf_reflow => {
                let blocks = pdf.page_blocks(spine).unwrap_or_else(|e| {
                    log::warn!("no text for pdf page {spine}: {e:?}");
                    Vec::new()
                });
                if blocks.is_empty() && settings.ocr {
                    ocr_layer(pdf, self.ocr_cache.as_ref(), spine)
                        .map(|l| l.blocks())
                        .unwrap_or_default()
                } else {
                    blocks
                }
            }
            Doc::Pdf(_) => Vec::new(),
        };
        let pages = if self.pdf().is_some() && blocks.is_empty() {
            vec![Page {
                lines: Vec::new(),
                start: Position::default(),
            }]
        } else {
            paginate(text, settings, self.language(), width, &blocks)
        };
        self.page = match target {
            Target::Start => 0,
            Target::End => pages.len() - 1,
            Target::At(pos) => page_of(&pages, pos),
        };
        self.spine = spine;
        self.blocks = blocks;
        self.pages = pages;
        Ok(())
    }

    /// Ayar değişince aynı konumu içeren sayfaya döner.
    pub fn relayout(&mut self, text: &Text, settings: &Settings, width: u32) {
        let at = self.position();
        if self.pdf().is_some() {
            // Yeniden akıtma açılıp kapanmış olabilir: sayfanın metnini yeniden oku.
            let spine = self.spine;
            let _ = self.load_chapter(text, settings, width, spine, Target::At(at));
            return;
        }
        self.pages = paginate(text, settings, self.language(), width, &self.blocks);
        self.page = page_of(&self.pages, at);
    }

    pub fn position(&self) -> Position {
        self.pages
            .get(self.page)
            .map_or(Position::default(), |p| p.start)
    }

    /// Kitap boyunca yaklaşık ilerleme: bölüm sırası + bölüm içindeki sayfa.
    pub fn percent(&self) -> u32 {
        let chapters = self.chapter_count().max(1) as f32;
        let within = (self.page + 1) as f32 / self.pages.len().max(1) as f32;
        (((self.spine as f32 + within) / chapters) * 100.0)
            .round()
            .min(100.0) as u32
    }

    /// Kayda yazılacak durumu günceller.
    pub fn record(&mut self) {
        self.state.last = Some((self.spine, self.position()));
        self.state.percent = self.percent();
    }

    /// Bu sayfaya düşen yer iminin sırası.
    pub fn bookmark_here(&self) -> Option<usize> {
        self.state
            .bookmarks
            .iter()
            .position(|b| b.spine == self.spine && page_of(&self.pages, b.position) == self.page)
    }

    /// Bu sayfada yer imi varsa kaldırır, yoksa ekler.
    pub fn toggle_bookmark(&mut self) {
        match self.bookmark_here() {
            Some(i) => {
                self.state.bookmarks.remove(i);
            }
            None => {
                let position = self.position();
                let snippet = self.snippet(position);
                self.state.bookmarks.push(Bookmark {
                    spine: self.spine,
                    position,
                    snippet,
                });
                self.state.bookmarks.sort_by_key(|b| (b.spine, b.position));
            }
        }
    }

    fn snippet(&self, at: Position) -> String {
        if self.pdf().is_some() {
            return tr(&Msg::PageNumber(self.spine + 1));
        }
        let mut out = String::new();
        for block in self.blocks.iter().skip(at.block) {
            let start = if out.is_empty() {
                at.offset.min(block.text.len())
            } else {
                0
            };
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&block.text[start..]);
            if out.chars().count() >= SNIPPET_CHARS {
                break;
            }
        }
        match out.char_indices().nth(SNIPPET_CHARS) {
            Some((cut, _)) => format!("{}…", out[..cut].trim_end()),
            None => out,
        }
    }
}

/// Metin katmanı olmayan PDF sayfasının OCR katmanı; önbellekte yoksa OCR yapılıp
/// yazılır. Hata olursa `None` (sayfa görüntüsü gösterilir).
pub(super) fn ocr_layer(pdf: &PdfDoc, cache: Option<&Store>, page: usize) -> Option<OcrLayer> {
    let key = format!("{page}.json");
    if let Some(layer) = cache.and_then(|c| c.load::<OcrLayer>(&key)) {
        return Some(layer);
    }
    let started = std::time::Instant::now();
    match ocr::ocr_pdf_page(pdf, page, "tur", OCR_DPI) {
        Ok(layer) => {
            log::info!(
                "OCR page {page}: skew {:.2}°, {} words in {:?}",
                layer.angle,
                layer.words.len(),
                started.elapsed()
            );
            if let Some(c) = cache {
                c.save(&key, &layer);
            }
            Some(layer)
        }
        Err(e) => {
            log::warn!("OCR failed on page {page}: {e:?}");
            None
        }
    }
}

/// Blokları sayfalara dizer. Her zaman en az bir (belki boş) sayfa döner.
fn paginate(
    text: &Text,
    s: &Settings,
    lang: Option<&str>,
    width: u32,
    blocks: &[Block],
) -> Vec<Page> {
    let family = Family::Book(s.family);
    let max_w = (width as i32 - 2 * s.margin) as f32;
    let body_line = s.font_px * s.line_spacing;

    let mut pages = vec![Page {
        lines: Vec::new(),
        start: Position::default(),
    }];
    let mut y = TOP as f32;
    for (index, block) in blocks.iter().enumerate() {
        let (px, align, heading) = match block.kind {
            BlockKind::Heading(1) => (s.font_px * 1.5, Align::Center, true),
            BlockKind::Heading(2) => (s.font_px * 1.3, Align::Center, true),
            BlockKind::Heading(_) => (s.font_px * 1.15, Align::Left, true),
            BlockKind::Paragraph => (
                s.font_px,
                if s.justify {
                    Align::Justify
                } else {
                    Align::Left
                },
                false,
            ),
        };
        let line_h = px * s.line_spacing;
        if heading && y > TOP as f32 {
            y += line_h * 0.6;
            if y + line_h + body_line * LINES_AFTER_HEADING > BOTTOM as f32 {
                y = BOTTOM as f32 + 1.0; // sonraki sayfaya zorla
            }
        }
        let face = text.fonts().face(
            family,
            Style {
                bold: heading,
                ..Style::default()
            },
        );
        let (ascent, descent) = text.ascent_descent(face, px);
        let para = ParaStyle {
            family,
            px,
            bold: heading,
            align,
            hyphenate: s.hyphenate && !heading,
            lang,
        };
        for line in layout(text, &block.text, &block.spans, &para, max_w) {
            if y + line_h > BOTTOM as f32 && !pages.last().is_some_and(|p| p.lines.is_empty()) {
                pages.push(Page {
                    lines: Vec::new(),
                    start: Position::default(),
                });
                y = TOP as f32;
            }
            let current = pages.last_mut().expect("en az bir sayfa var");
            if current.lines.is_empty() {
                current.start = Position {
                    block: index,
                    offset: line.start,
                };
            }
            let baseline = (y + (line_h - (ascent + descent)) / 2.0 + ascent) as i32;
            current.lines.push(PlacedLine { line, baseline });
            y += line_h;
        }
        y += body_line * 0.35;
    }
    pages
}

/// `at` konumunu içeren sayfa: başlangıcı `at`'ten büyük olmayan son sayfa.
pub(super) fn page_of(pages: &[Page], at: Position) -> usize {
    pages.partition_point(|p| p.start <= at).saturating_sub(1)
}
