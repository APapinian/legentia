use crate::LIBRA2_HEIGHT;
use crate::canvas::Canvas;
use crate::epub::{Block, BlockKind, Book, EpubError};
use crate::event::{Event, PageKey, Refresh};
use crate::i18n::{Msg, tr};
use crate::text::{Align, Family, Line, ParaStyle, Style, Text, layout};

/// Kaç kısmi yenilemeden sonra bir tam yenileme yapılacağı.
/// Geçici değer; gerçek cihazda gölgelenmeye bakılarak ayarlanacak.
const PARTIALS_PER_FULL: u32 = 6;

// Sayfa iskeleti (piksel, 300 ppi). Metin alanının yan boşlukları ayarlardan gelir.
const TOP: i32 = 90;
const BOTTOM: i32 = LIBRA2_HEIGHT as i32 - 110;
const FOOTER_BASELINE: i32 = LIBRA2_HEIGHT as i32 - 50;
const UI_MARGIN: i32 = 80;
const FOOTER_PX: f32 = 28.0;
const TOC_PX: f32 = 36.0;
const TOC_ROW: i32 = 64;
const TOC_INDENT: i32 = 40;
/// Bir başlığın altında en az bu kadar satır yer yoksa başlık sonraki sayfaya geçer.
const LINES_AFTER_HEADING: f32 = 3.0;

/// Okuma ayarları. Değişince bölüm yeniden dizilir, okuma konumu korunur.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// `text::book_family_name` sırası.
    pub family: usize,
    /// Gövde metni boyutu (piksel; 300 ppi'de 40 px ≈ 9,6 punto).
    pub font_px: f32,
    pub line_spacing: f32,
    /// Sol ve sağ kenar boşluğu (piksel).
    pub margin: i32,
    pub justify: bool,
    pub hyphenate: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            family: 0,
            font_px: 40.0,
            line_spacing: 1.4,
            margin: 70,
            justify: true,
            hyphenate: true,
        }
    }
}

/// Bölüm içinde kararlı konum: blok sırası ve blok metnindeki bayt konumu.
/// Sayfa numarasından farklı olarak yazı ayarı değişince de geçerlidir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Position {
    pub block: usize,
    pub offset: usize,
}

struct PlacedLine {
    line: Line,
    baseline: i32,
}

struct Page {
    lines: Vec<PlacedLine>,
    start: Position,
}

enum View {
    Message(Msg),
    Contents {
        page: usize,
    },
    Reading {
        spine: usize,
        blocks: Vec<Block>,
        pages: Vec<Page>,
        page: usize,
    },
}

pub struct App {
    text: Text,
    width: u32,
    book: Option<Book>,
    view: View,
    settings: Settings,
    since_full: u32,
}

impl App {
    /// `book` açılamadıysa hata ekranıyla başlar.
    pub fn new(text: Text, width: u32, book: Result<Book, EpubError>) -> Self {
        let (book, view) = match book {
            Ok(b) => (Some(b), View::Contents { page: 0 }),
            Err(e) => {
                log::error!("cannot open book: {e:?}");
                (None, View::Message(e.message()))
            }
        };
        Self {
            text,
            width,
            book,
            view,
            settings: Settings::default(),
            since_full: 0,
        }
    }

    /// Kitapsız, yalnızca bir mesaj gösteren uygulama.
    pub fn with_message(text: Text, width: u32, msg: Msg) -> Self {
        Self {
            text,
            width,
            book: None,
            view: View::Message(msg),
            settings: Settings::default(),
            since_full: 0,
        }
    }

    /// İlk açılışta ekran her zaman tam yenilenir.
    pub fn start(&mut self) -> Refresh {
        self.refresh(true)
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Ayarları uygular. Okurken bölüm yeniden dizilir ve aynı metin
    /// konumunu içeren sayfa açılır.
    pub fn set_settings(&mut self, settings: Settings) -> Option<Refresh> {
        if settings == self.settings {
            return None;
        }
        self.settings = settings;
        let View::Reading {
            blocks,
            pages,
            page,
            ..
        } = &mut self.view
        else {
            return None;
        };
        let at = pages[*page].start;
        let blocks = std::mem::take(blocks);
        let new_pages = self.paginate(&blocks);
        let new_page = page_of(&new_pages, at);
        if let View::Reading {
            blocks: b,
            pages,
            page,
            ..
        } = &mut self.view
        {
            *b = blocks;
            *pages = new_pages;
            *page = new_page;
        }
        Some(self.refresh(true))
    }

    /// Okunan sayfanın başlangıç konumu (bölüm, konum).
    pub fn position(&self) -> Option<(usize, Position)> {
        match &self.view {
            View::Reading {
                spine, pages, page, ..
            } => Some((*spine, pages[*page].start)),
            _ => None,
        }
    }

    /// Bir olayı işler; ekran değişmediyse `None` döner (E Ink'e boşuna yenileme yok).
    pub fn handle(&mut self, event: Event) -> Option<Refresh> {
        match self.view {
            View::Message(_) => {
                if self.book.is_some() && matches!(event, Event::Tap { .. }) {
                    self.view = View::Contents { page: 0 };
                    return Some(self.refresh(true));
                }
                None
            }
            View::Contents { page } => self.handle_contents(page, event),
            View::Reading {
                spine,
                page,
                ref pages,
                ..
            } => {
                let last = pages.len() - 1;
                self.handle_reading(spine, page, last, event)
            }
        }
    }

    /// Sayfa tuşları listeyi çevirir, dokunma satırı açar.
    fn handle_contents(&mut self, page: usize, event: Event) -> Option<Refresh> {
        let new_page = match event {
            Event::Key(PageKey::Forward) if page + 1 < self.contents_pages() => page + 1,
            Event::Key(PageKey::Back) if page > 0 => page - 1,
            Event::Tap { y, .. } => {
                let index = self.contents_row_at(page, y)?;
                self.open_chapter(self.contents_spine(index), Target::Start);
                return Some(self.refresh(true));
            }
            Event::Key(_) => return None,
        };
        self.view = View::Contents { page: new_page };
        Some(self.refresh(false))
    }

    /// Sol üçte bir ve geri tuşu: önceki sayfa; sağ üçte bir ve ileri tuşu:
    /// sonraki sayfa; orta: içindekiler. Bölüm sınırında komşu bölüme geçer.
    fn handle_reading(
        &mut self,
        spine: usize,
        page: usize,
        last: usize,
        event: Event,
    ) -> Option<Refresh> {
        let third = self.width / 3;
        let step = match event {
            Event::Key(PageKey::Forward) => Step::Forward,
            Event::Key(PageKey::Back) => Step::Back,
            Event::Tap { x, .. } if x < third => Step::Back,
            Event::Tap { x, .. } if x > 2 * third => Step::Forward,
            Event::Tap { .. } => Step::Contents,
        };
        let new_page = match step {
            Step::Forward if page < last => page + 1,
            Step::Back if page > 0 => page - 1,
            Step::Forward if spine + 1 < self.chapter_count() => {
                self.open_chapter(spine + 1, Target::Start);
                return Some(self.refresh(true));
            }
            Step::Back if spine > 0 => {
                self.open_chapter(spine - 1, Target::End);
                return Some(self.refresh(true));
            }
            Step::Forward | Step::Back => return None,
            Step::Contents => {
                let row = self.contents_row_of_spine(spine);
                self.view = View::Contents {
                    page: row / self.rows_per_contents_page(),
                };
                return Some(self.refresh(true));
            }
        };
        if let View::Reading { page, .. } = &mut self.view {
            *page = new_page;
        }
        Some(self.refresh(false))
    }

    fn refresh(&mut self, force_full: bool) -> Refresh {
        if force_full || self.since_full + 1 >= PARTIALS_PER_FULL {
            self.since_full = 0;
            Refresh::Full
        } else {
            self.since_full += 1;
            Refresh::Partial
        }
    }

    fn chapter_count(&self) -> usize {
        self.book.as_ref().map_or(0, |b| b.spine.len())
    }

    // --- İçindekiler ---

    /// İçindekiler satır sayısı: kitabın kendi listesi, yoksa her spine öğesi.
    fn contents_len(&self) -> usize {
        match &self.book {
            Some(b) if !b.toc.is_empty() => b.toc.len(),
            Some(b) => b.spine.len(),
            None => 0,
        }
    }

    fn contents_spine(&self, index: usize) -> usize {
        match &self.book {
            Some(b) if !b.toc.is_empty() => b.toc[index].spine_index,
            _ => index,
        }
    }

    fn contents_row_of_spine(&self, spine: usize) -> usize {
        match &self.book {
            Some(b) if !b.toc.is_empty() => b
                .toc
                .iter()
                .rposition(|t| t.spine_index <= spine)
                .unwrap_or(0),
            _ => spine,
        }
    }

    fn contents_label(&self, index: usize) -> (String, u32) {
        match &self.book {
            Some(b) if !b.toc.is_empty() => (b.toc[index].title.clone(), b.toc[index].depth),
            _ => (tr(&Msg::ChapterNumber(index + 1)), 0),
        }
    }

    /// İçindekiler listesinin ilk satırının üst kenarı (başlık bloğunun altı).
    fn contents_list_top(&self) -> i32 {
        TOP + 240
    }

    fn rows_per_contents_page(&self) -> usize {
        ((BOTTOM - self.contents_list_top()) / TOC_ROW).max(1) as usize
    }

    fn contents_pages(&self) -> usize {
        self.contents_len()
            .div_ceil(self.rows_per_contents_page())
            .max(1)
    }

    fn contents_row_at(&self, page: usize, y: u32) -> Option<usize> {
        let top = self.contents_list_top();
        let y = y as i32;
        if y < top {
            return None;
        }
        let index = page * self.rows_per_contents_page() + ((y - top) / TOC_ROW) as usize;
        let on_page = index < (page + 1) * self.rows_per_contents_page();
        (on_page && index < self.contents_len()).then_some(index)
    }

    // --- Okuma ---

    fn open_chapter(&mut self, spine: usize, target: Target) {
        let Some(book) = self.book.as_mut() else {
            return;
        };
        match book.chapter(spine) {
            Ok(blocks) => {
                let pages = self.paginate(&blocks);
                let page = match target {
                    Target::Start => 0,
                    Target::End => pages.len() - 1,
                };
                self.view = View::Reading {
                    spine,
                    blocks,
                    pages,
                    page,
                };
            }
            Err(e) => {
                log::error!("cannot read chapter {spine}: {e:?}");
                self.view = View::Message(e.message());
            }
        }
    }

    /// Blokları sayfalara dizer. Her zaman en az bir (belki boş) sayfa döner.
    fn paginate(&self, blocks: &[Block]) -> Vec<Page> {
        let s = &self.settings;
        let family = Family::Book(s.family);
        let lang = self.book.as_ref().and_then(|b| b.language.as_deref());
        let max_w = (self.width as i32 - 2 * s.margin) as f32;
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
            let face = self.text.fonts().face(
                family,
                Style {
                    bold: heading,
                    ..Style::default()
                },
            );
            let (ascent, descent) = self.text.ascent_descent(face, px);
            let para = ParaStyle {
                family,
                px,
                bold: heading,
                align,
                hyphenate: s.hyphenate && !heading,
                lang,
            };
            for line in layout(&self.text, &block.text, &block.spans, &para, max_w) {
                let current = pages.last_mut().expect("en az bir sayfa var");
                if y + line_h > BOTTOM as f32 && !current.lines.is_empty() {
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

    // --- Çizim ---

    pub fn draw(&self, canvas: &mut Canvas) {
        canvas.fill(255);
        let t = &self.text;
        match &self.view {
            View::Message(msg) => {
                let max_w = (canvas.width() as i32 - 2 * UI_MARGIN) as f32;
                let mut y = TOP + 300;
                for line in t.wrap_plain(&tr(msg), Family::Ui, Style::default(), 40.0, max_w) {
                    t.draw_plain(
                        canvas,
                        &line,
                        Family::Ui,
                        Style::default(),
                        UI_MARGIN,
                        y,
                        40.0,
                    );
                    y += 58;
                }
                if self.book.is_some() {
                    t.draw_msg(canvas, &Msg::TapToReturn, UI_MARGIN, y + 60, FOOTER_PX);
                }
            }
            View::Contents { page } => self.draw_contents(canvas, *page),
            View::Reading {
                spine, pages, page, ..
            } => {
                let current = &pages[*page];
                for placed in &current.lines {
                    t.draw_line(canvas, &placed.line, self.settings.margin, placed.baseline);
                }
                if current.lines.is_empty() {
                    t.draw_msg(canvas, &Msg::EmptyChapter, UI_MARGIN, TOP + 300, 36.0);
                }
                let footer = Msg::ReadingFooter {
                    chapter: spine + 1,
                    chapters: self.chapter_count(),
                    page: page + 1,
                    pages: pages.len(),
                };
                t.draw_msg(
                    canvas,
                    &footer,
                    self.settings.margin,
                    FOOTER_BASELINE,
                    FOOTER_PX,
                );
            }
        }
    }

    fn draw_contents(&self, canvas: &mut Canvas, page: usize) {
        let Some(book) = &self.book else {
            return;
        };
        let t = &self.text;
        let ui = Family::Ui;
        let bold = Style {
            bold: true,
            ..Style::default()
        };
        let max_w = (canvas.width() as i32 - 2 * UI_MARGIN) as f32;

        let title = book.title.clone().unwrap_or_else(|| tr(&Msg::UntitledBook));
        let first_line = t
            .wrap_plain(&title, ui, bold, 56.0, max_w)
            .into_iter()
            .next()
            .unwrap_or_default();
        t.draw_plain(canvas, &first_line, ui, bold, UI_MARGIN, TOP + 60, 56.0);
        let authors = if book.authors.is_empty() {
            tr(&Msg::UnknownAuthor)
        } else {
            book.authors.join(", ")
        };
        t.draw_plain(
            canvas,
            &authors,
            ui,
            Style::default(),
            UI_MARGIN,
            TOP + 120,
            36.0,
        );
        t.draw_plain(
            canvas,
            &tr(&Msg::Contents),
            ui,
            bold,
            UI_MARGIN,
            TOP + 205,
            40.0,
        );
        canvas.fill_rect(
            UI_MARGIN,
            TOP + 222,
            canvas.width() - 2 * UI_MARGIN as u32,
            3,
            0,
        );

        let per_page = self.rows_per_contents_page();
        let start = page * per_page;
        let end = (start + per_page).min(self.contents_len());
        for (row, index) in (start..end).enumerate() {
            let (label, depth) = self.contents_label(index);
            let x = UI_MARGIN + depth.min(4) as i32 * TOC_INDENT;
            let avail = (canvas.width() as i32 - UI_MARGIN - x) as f32;
            let lines = t.wrap_plain(&label, ui, Style::default(), TOC_PX, avail - TOC_PX);
            let mut text = lines.first().cloned().unwrap_or_default();
            if lines.len() > 1 {
                text.push('…');
            }
            let baseline = self.contents_list_top() + row as i32 * TOC_ROW + 46;
            t.draw_plain(canvas, &text, ui, Style::default(), x, baseline, TOC_PX);
        }
        let footer = Msg::ContentsFooter {
            page: page + 1,
            pages: self.contents_pages(),
        };
        t.draw_msg(canvas, &footer, UI_MARGIN, FOOTER_BASELINE, FOOTER_PX);
    }
}

/// `at` konumunu içeren sayfa: başlangıcı `at`'ten büyük olmayan son sayfa.
fn page_of(pages: &[Page], at: Position) -> usize {
    pages.partition_point(|p| p.start <= at).saturating_sub(1)
}

enum Target {
    Start,
    End,
}

enum Step {
    Forward,
    Back,
    Contents,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epub::testutil::*;
    use crate::text::test_text;
    use crate::{LIBRA2_WIDTH, epub};

    fn app_with(bytes: Vec<u8>) -> App {
        App::new(test_text(), LIBRA2_WIDTH, open_bytes(bytes))
    }

    fn long_chapter(paragraphs: usize) -> App {
        let para = "<p>Uzun bir paragraf: ağaçlar, gölgeler, ışık ve çay; kardeşlerimizden biri İstanbul'dan geldi. </p>"
            .repeat(paragraphs);
        let mut app = app_with(epub_with_chapter(&format!(
            "<html><body><h1>Başlık</h1>{para}</body></html>"
        )));
        app.open_chapter(0, Target::Start);
        app
    }

    fn tap_row(app: &mut App, row: i32) -> Option<Refresh> {
        let y = app.contents_list_top() + row * TOC_ROW + 20;
        app.handle(Event::Tap {
            x: LIBRA2_WIDTH / 2,
            y: y as u32,
        })
    }

    fn pages(app: &App) -> &[Page] {
        match &app.view {
            View::Reading { pages, .. } => pages,
            _ => panic!("okuma ekranı bekleniyordu"),
        }
    }

    #[test]
    fn opens_on_contents_and_taps_into_a_chapter() {
        let mut app = app_with(epub3_sample());
        assert!(matches!(app.view, View::Contents { page: 0 }));
        assert_eq!(tap_row(&mut app, 2), Some(Refresh::Full));
        assert!(matches!(
            app.view,
            View::Reading {
                spine: 1,
                page: 0,
                ..
            }
        ));
    }

    #[test]
    fn tap_below_last_row_does_nothing() {
        let mut app = app_with(epub3_sample());
        assert_eq!(tap_row(&mut app, 10), None);
    }

    #[test]
    fn page_keys_cross_chapter_boundaries() {
        let mut app = app_with(epub3_sample());
        tap_row(&mut app, 0);
        app.handle(Event::Key(PageKey::Forward));
        assert!(matches!(app.view, View::Reading { spine: 1, .. }));
        app.handle(Event::Key(PageKey::Back));
        assert!(matches!(app.view, View::Reading { spine: 0, .. }));
        assert_eq!(
            app.handle(Event::Key(PageKey::Back)),
            None,
            "kitabın başında geri gidilmez"
        );
    }

    #[test]
    fn long_chapter_is_split_into_pages_that_fit() {
        let app = long_chapter(300);
        assert!(pages(&app).len() > 5);
        for page in pages(&app) {
            assert!(page.lines.iter().all(|l| l.baseline <= BOTTOM));
            assert!(!page.lines.is_empty());
        }
    }

    #[test]
    fn page_starts_are_strictly_increasing() {
        let app = long_chapter(200);
        for w in pages(&app).windows(2) {
            assert!(w[0].start < w[1].start);
        }
    }

    #[test]
    fn changing_settings_keeps_the_reading_position() {
        let mut app = long_chapter(300);
        for _ in 0..7 {
            app.handle(Event::Key(PageKey::Forward));
        }
        let (_, before) = app.position().unwrap();
        let bigger = Settings {
            font_px: 56.0,
            family: 1,
            margin: 120,
            ..app.settings().clone()
        };
        assert_eq!(app.set_settings(bigger), Some(Refresh::Full));
        let View::Reading { pages, page, .. } = &app.view else {
            panic!()
        };
        assert!(pages[*page].start <= before);
        assert!(
            pages.get(*page + 1).is_none_or(|next| before < next.start),
            "konum yeni sayfada değil"
        );
        assert!(pages.len() > 10, "büyük yazıyla daha çok sayfa beklenir");
    }

    #[test]
    fn same_settings_do_not_refresh() {
        let mut app = long_chapter(10);
        let same = app.settings().clone();
        assert_eq!(app.set_settings(same), None);
    }

    #[test]
    fn middle_tap_returns_to_contents() {
        let mut app = app_with(epub3_sample());
        tap_row(&mut app, 1);
        app.handle(Event::Tap {
            x: LIBRA2_WIDTH / 2,
            y: 800,
        });
        assert!(matches!(app.view, View::Contents { .. }));
    }

    #[test]
    fn broken_book_shows_turkish_error() {
        let app = App::new(
            test_text(),
            LIBRA2_WIDTH,
            Err(epub::EpubError::NotZip("x".into())),
        );
        assert!(matches!(&app.view, View::Message(Msg::BookNotEpub)));
        let mut c = Canvas::new(LIBRA2_WIDTH, LIBRA2_HEIGHT);
        app.draw(&mut c);
    }

    #[test]
    fn every_sixth_page_turn_is_a_full_refresh() {
        let mut app = long_chapter(400);
        let kinds: Vec<_> = (0..12)
            .map(|_| app.handle(Event::Key(PageKey::Forward)).unwrap())
            .collect();
        let fulls: Vec<_> = kinds
            .iter()
            .enumerate()
            .filter(|(_, r)| **r == Refresh::Full)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(fulls, vec![5, 11]);
    }
}
