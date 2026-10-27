use crate::LIBRA2_HEIGHT;
use crate::canvas::Canvas;
use crate::epub::{BlockKind, Book, EpubError};
use crate::event::{Event, PageKey, Refresh};
use crate::i18n::{Msg, tr};
use crate::text::Font;

/// Kaç kısmi yenilemeden sonra bir tam yenileme yapılacağı.
/// Geçici değer; gerçek cihazda gölgelenmeye bakılarak ayarlanacak.
const PARTIALS_PER_FULL: u32 = 6;

// Geçici sayfa düzeni (piksel, 300 ppi). Ayarlanabilir hali M3/M4'te.
const MARGIN: i32 = 80;
const TOP: i32 = 110;
const BOTTOM: i32 = LIBRA2_HEIGHT as i32 - 120;
const FOOTER_BASELINE: i32 = LIBRA2_HEIGHT as i32 - 55;
const BODY_PX: f32 = 34.0;
const LINE_SPACING: f32 = 1.45;
const FOOTER_PX: f32 = 28.0;
const TOC_PX: f32 = 36.0;
const TOC_ROW: i32 = 64;
const TOC_INDENT: i32 = 40;

struct Line {
    text: String,
    px: f32,
    baseline: i32,
}

type Page = Vec<Line>;

enum View {
    Message(Msg),
    Contents {
        page: usize,
    },
    Reading {
        spine: usize,
        pages: Vec<Page>,
        page: usize,
    },
}

pub struct App {
    font: Font,
    width: u32,
    book: Option<Book>,
    view: View,
    since_full: u32,
}

impl App {
    /// `book` açılamadıysa hata ekranıyla başlar.
    pub fn new(font: Font, width: u32, book: Result<Book, EpubError>) -> Self {
        let (book, view) = match book {
            Ok(b) => (Some(b), View::Contents { page: 0 }),
            Err(e) => {
                log::error!("cannot open book: {e:?}");
                (None, View::Message(e.message()))
            }
        };
        Self {
            font,
            width,
            book,
            view,
            since_full: 0,
        }
    }

    /// Kitapsız, yalnızca bir mesaj gösteren uygulama.
    pub fn with_message(font: Font, width: u32, msg: Msg) -> Self {
        Self {
            font,
            width,
            book: None,
            view: View::Message(msg),
            since_full: 0,
        }
    }

    /// İlk açılışta ekran her zaman tam yenilenir.
    pub fn start(&mut self) -> Refresh {
        self.refresh(true)
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
                self.open_chapter(self.contents_spine(index), false);
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
                self.open_chapter(spine + 1, false);
                return Some(self.refresh(true));
            }
            Step::Back if spine > 0 => {
                self.open_chapter(spine - 1, true);
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
        TOP + 220
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

    fn open_chapter(&mut self, spine: usize, at_end: bool) {
        let Some(book) = self.book.as_mut() else {
            return;
        };
        match book.chapter(spine) {
            Ok(blocks) => {
                let pages = self.paginate(&blocks);
                let page = if at_end { pages.len() - 1 } else { 0 };
                self.view = View::Reading { spine, pages, page };
            }
            Err(e) => {
                log::error!("cannot read chapter {spine}: {e:?}");
                self.view = View::Message(e.message());
            }
        }
    }

    fn paginate(&self, blocks: &[crate::epub::Block]) -> Vec<Page> {
        let max_w = (self.width as i32 - 2 * MARGIN) as f32;
        let mut pages = vec![Vec::new()];
        let mut y = TOP as f32;
        for block in blocks {
            let px = match block.kind {
                BlockKind::Heading(1) => 52.0,
                BlockKind::Heading(2) => 46.0,
                BlockKind::Heading(_) => 40.0,
                BlockKind::Paragraph => BODY_PX,
            };
            let line_h = px * LINE_SPACING;
            if matches!(block.kind, BlockKind::Heading(_)) && y > TOP as f32 {
                y += line_h * 0.6;
            }
            for text in self.font.wrap(&block.text, px, max_w) {
                if y + line_h > BOTTOM as f32 && !pages.last().is_some_and(Vec::is_empty) {
                    pages.push(Vec::new());
                    y = TOP as f32;
                }
                let baseline = (y + px) as i32;
                pages
                    .last_mut()
                    .expect("en az bir sayfa var")
                    .push(Line { text, px, baseline });
                y += line_h;
            }
            y += BODY_PX * 0.5;
        }
        pages
    }

    // --- Çizim ---

    pub fn draw(&self, canvas: &mut Canvas) {
        canvas.fill(255);
        let f = &self.font;
        match &self.view {
            View::Message(msg) => {
                let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
                let mut y = TOP + 300;
                for line in f.wrap(&tr(msg), 40.0, max_w) {
                    f.draw_content(canvas, &line, MARGIN, y, 40.0);
                    y += 58;
                }
                if self.book.is_some() {
                    f.draw_msg(canvas, &Msg::TapToReturn, MARGIN, y + 60, FOOTER_PX);
                }
            }
            View::Contents { page } => self.draw_contents(canvas, *page),
            View::Reading { spine, pages, page } => {
                for line in &pages[*page] {
                    f.draw_content(canvas, &line.text, MARGIN, line.baseline, line.px);
                }
                if pages[*page].is_empty() {
                    f.draw_msg(canvas, &Msg::EmptyChapter, MARGIN, TOP + 300, BODY_PX);
                }
                let footer = Msg::ReadingFooter {
                    chapter: spine + 1,
                    chapters: self.chapter_count(),
                    page: page + 1,
                    pages: pages.len(),
                };
                f.draw_msg(canvas, &footer, MARGIN, FOOTER_BASELINE, FOOTER_PX);
            }
        }
    }

    fn draw_contents(&self, canvas: &mut Canvas, page: usize) {
        let Some(book) = &self.book else {
            return;
        };
        let f = &self.font;
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;

        let title = book.title.clone().unwrap_or_else(|| tr(&Msg::UntitledBook));
        let first_line = f
            .wrap(&title, 52.0, max_w)
            .into_iter()
            .next()
            .unwrap_or_default();
        f.draw_content(canvas, &first_line, MARGIN, TOP + 50, 52.0);
        let authors = if book.authors.is_empty() {
            tr(&Msg::UnknownAuthor)
        } else {
            book.authors.join(", ")
        };
        f.draw_content(canvas, &authors, MARGIN, TOP + 110, 34.0);
        f.draw_msg(canvas, &Msg::Contents, MARGIN, TOP + 185, 40.0);
        canvas.fill_rect(MARGIN, TOP + 200, canvas.width() - 2 * MARGIN as u32, 3, 0);

        let per_page = self.rows_per_contents_page();
        let start = page * per_page;
        let end = (start + per_page).min(self.contents_len());
        for (row, index) in (start..end).enumerate() {
            let (label, depth) = self.contents_label(index);
            let x = MARGIN + depth.min(4) as i32 * TOC_INDENT;
            let avail = (canvas.width() as i32 - MARGIN - x) as f32;
            let mut text = f
                .wrap(&label, TOC_PX, avail)
                .into_iter()
                .next()
                .unwrap_or_default();
            if text.chars().count() < label.chars().count() {
                text.push('…');
            }
            let baseline = self.contents_list_top() + row as i32 * TOC_ROW + 44;
            f.draw_content(canvas, &text, x, baseline, TOC_PX);
        }
        let footer = Msg::ContentsFooter {
            page: page + 1,
            pages: self.contents_pages(),
        };
        f.draw_msg(canvas, &footer, MARGIN, FOOTER_BASELINE, FOOTER_PX);
    }
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
    use crate::text::test_font;
    use crate::{LIBRA2_WIDTH, epub};

    fn app_with(bytes: Vec<u8>) -> App {
        App::new(test_font(), LIBRA2_WIDTH, open_bytes(bytes))
    }

    fn tap_row(app: &mut App, row: i32) -> Option<Refresh> {
        let y = app.contents_list_top() + row * TOC_ROW + 20;
        app.handle(Event::Tap {
            x: LIBRA2_WIDTH / 2,
            y: y as u32,
        })
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
        let para = "<p>Uzun bir paragraf: ağaçlar, gölgeler, ışık ve çay. </p>".repeat(400);
        let mut app = app_with(epub_with_chapter(&format!(
            "<html><body>{para}</body></html>"
        )));
        app.open_chapter(0, false);
        let View::Reading { pages, .. } = &app.view else {
            panic!("okuma ekranı bekleniyordu")
        };
        assert!(pages.len() > 5);
        for page in pages {
            assert!(page.iter().all(|l| l.baseline <= BOTTOM));
        }
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
            test_font(),
            LIBRA2_WIDTH,
            Err(epub::EpubError::NotZip("x".into())),
        );
        assert!(matches!(&app.view, View::Message(Msg::BookNotEpub)));
        let mut c = Canvas::new(LIBRA2_WIDTH, LIBRA2_HEIGHT);
        app.draw(&mut c);
    }

    #[test]
    fn every_sixth_page_turn_is_a_full_refresh() {
        let para = "<p>Sayfa sayfa metin. </p>".repeat(2000);
        let mut app = app_with(epub_with_chapter(&format!(
            "<html><body>{para}</body></html>"
        )));
        app.open_chapter(0, false);
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
