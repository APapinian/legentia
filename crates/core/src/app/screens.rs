//! Ekranlar: her biri için olay işleme ve çizim.

use super::session::Target;
use super::settings::{FIELD_PAGES, Field, PAGE_TITLES, Settings, TapZones};
use super::ui::{
    BOLD, BOTTOM, HEADER_BOTTOM, List, MARGIN, PLAIN, ROW_PX, Rect, TOP, back_button, bottom_of,
    draw_button, draw_footer, draw_header, draw_header_until, header_button, one_line,
};
use super::{App, Back, View};
use crate::canvas::Canvas;
use crate::event::{Event, PageKey, Refresh};
use crate::i18n::{Msg, tr};
use crate::library::Format;
use crate::text::{Family, Style};

pub(super) const LIBRARY: List = List {
    top: HEADER_BOTTOM + super::shelf::TOOLBAR_H,
    row_h: 120,
};
pub(super) const CONTENTS: List = List {
    top: HEADER_BOTTOM,
    row_h: 66,
};
pub(super) const BOOKMARKS: List = List {
    top: HEADER_BOTTOM,
    row_h: 120,
};
pub(super) const WORDS: List = List {
    top: HEADER_BOTTOM,
    row_h: 120,
};
pub(super) const SEARCH: List = List {
    top: HEADER_BOTTOM,
    row_h: 120,
};
pub(super) const ANNOTATIONS: List = List {
    top: HEADER_BOTTOM,
    row_h: 120,
};
const TOC_INDENT: i32 = 40;
/// Okuma menüsünün çubuklarındaki düğme yüksekliği, aralık ve üst çubuğun
/// yan düğmelerinin genişliği.
const BAR_ROW: i32 = 100;
const BAR_GAP: i32 = 20;
const BAR_SIDE_W: i32 = 330;
const BAR_PX: f32 = 32.0;
/// Üst çubuğun ikinci satırı ve alt çubuk.
const NAV_ITEMS: [MenuItem; 5] = [
    MenuItem::Contents,
    MenuItem::Search,
    MenuItem::Bookmarks,
    MenuItem::Annotations,
    MenuItem::Words,
];
const ACTION_ITEMS: [MenuItem; 4] = [
    MenuItem::Settings,
    MenuItem::AutoTurn,
    MenuItem::ExportNotes,
    MenuItem::SaveSearchable,
];
const SETTINGS_ROW: i32 = 128;
const STEP_BUTTON: i32 = 100;
/// Kütüphane başlığındaki "Kataloglar" düğmesinin genişliği.
pub(super) const CATALOGS_BUTTON_W: i32 = 230;

/// Kütüphane başlığında "Kataloglar"ın solundaki "Çık" düğmesi.
pub(super) fn exit_button(width: u32) -> Rect {
    super::ui::left_of(header_button(width, CATALOGS_BUTTON_W), 130)
}

/// Okurken açılan menünün seçenekleri.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MenuItem {
    Contents,
    Search,
    Bookmark,
    Bookmarks,
    Annotations,
    ExportNotes,
    Words,
    Settings,
    SaveSearchable,
    AutoTurn,
    Library,
}

impl App {
    /// Menünün seçenekleri; "Aranabilir PDF olarak kaydet" yalnızca PDF'te.
    pub(super) fn menu_items(&self) -> Vec<MenuItem> {
        let mut items = vec![
            MenuItem::Contents,
            MenuItem::Search,
            MenuItem::Bookmark,
            MenuItem::Bookmarks,
            MenuItem::Annotations,
            MenuItem::ExportNotes,
            MenuItem::Words,
            MenuItem::Settings,
            MenuItem::AutoTurn,
        ];
        if self.session.as_ref().is_some_and(|s| s.pdf().is_some()) {
            items.push(MenuItem::SaveSearchable);
        }
        items.push(MenuItem::Library);
        items
    }

    // --- Ortak ---

    /// Liste ekranlarında sayfa tuşları: listeyi çevirir; uçta `None`.
    /// Liste ekranlarında sayfa tuşları ve yatay kaydırma: listeyi çevirir; uçta `None`.
    pub(super) fn page_list(page: usize, pages: usize, event: Event) -> Option<usize> {
        match (event, swipe_direction(event)) {
            (Event::Key(PageKey::Forward), _) | (_, Some(true)) if page + 1 < pages => {
                Some(page + 1)
            }
            (Event::Key(PageKey::Back), _) | (_, Some(false)) if page > 0 => Some(page - 1),
            _ => None,
        }
    }

    pub(super) fn draw_message(&self, canvas: &mut Canvas, msg: &Msg) {
        let t = &self.text;
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
        let mut y = TOP + 300;
        for line in t.wrap_plain(&tr(msg), Family::Ui, PLAIN, 40.0, max_w) {
            t.draw_plain(canvas, &line, Family::Ui, PLAIN, MARGIN, y, 40.0);
            y += 58;
        }
        t.draw_msg(canvas, &Msg::TapToReturn, MARGIN, y + 60, ROW_PX);
    }

    // --- Kütüphane ---

    pub(super) fn handle_library(&mut self, page: usize, event: Event) -> Option<Refresh> {
        if let Event::Tap { x, y } = event {
            if back_button(self.width).contains(x, y) {
                self.view = View::Backup;
                return Some(self.refresh(true));
            }
            if header_button(self.width, CATALOGS_BUTTON_W).contains(x, y) {
                return self.open_catalogs();
            }
            if exit_button(self.width).contains(x, y) {
                self.shutdown();
                self.exit = true;
                return None;
            }
            if let Some(refresh) = self.handle_toolbar(x, y) {
                return Some(refresh);
            }
            let books = self.visible_books();
            let row = self.library_index_at(&books, page, x, y)?;
            let path = self.library[books[row]].path.clone();
            self.open_book(&path);
            return Some(self.refresh(true));
        }
        if let Event::Hold { x, y } = event {
            return self.open_book_actions(page, x, y);
        }
        let page = Self::page_list(page, self.library_pages(&self.visible_books()), event)?;
        self.view = View::Library { page };
        Some(self.refresh(false))
    }

    pub(super) fn draw_library(&self, canvas: &mut Canvas, page: usize) {
        let t = &self.text;
        let books = self.visible_books();
        let catalogs = header_button(canvas.width(), CATALOGS_BUTTON_W);
        let exit = exit_button(canvas.width());
        draw_header_until(
            t,
            canvas,
            &tr(&Msg::Library),
            &tr(&if books.len() == self.library.len() {
                Msg::BookCount(books.len())
            } else {
                Msg::BookCountShown {
                    shown: books.len(),
                    total: self.library.len(),
                }
            }),
            exit.x - 30,
        );
        draw_button(t, canvas, exit, &tr(&Msg::Exit), ROW_PX);
        // Başlıktaki "Geri" düğmesinin yerinde yedekleme ekranı düğmesi, solunda kataloglar.
        draw_button(
            t,
            canvas,
            back_button(canvas.width()),
            &tr(&Msg::BackupButton),
            ROW_PX,
        );
        draw_button(t, canvas, catalogs, &tr(&Msg::Catalogs), ROW_PX);
        self.draw_toolbar(canvas);
        if self.library.is_empty() {
            self.draw_message_lines(canvas, &Msg::LibraryEmpty, LIBRARY.top + 80);
        } else if books.is_empty() {
            self.draw_message_lines(canvas, &Msg::NoMatchingBooks, LIBRARY.top + 80);
        }
        let footer = Msg::ListFooter {
            page: page + 1,
            pages: self.library_pages(&books),
        };
        draw_footer(t, canvas, &footer, MARGIN);
        match self.settings.library_view {
            super::shelf::LibraryView::List => {}
            super::shelf::LibraryView::Covers => return self.draw_grid(canvas, &books, page),
            super::shelf::LibraryView::Shelves => {
                return self.draw_bookcase(canvas, &books, page);
            }
        }
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
        for (index, row) in LIBRARY.visible(page, books.len()) {
            let entry = &self.library[books[index]];
            let r = LIBRARY.row_rect(canvas.width(), row);
            let title = one_line(t, &entry.display_title(), BOLD, ROW_PX, max_w);
            t.draw_plain(canvas, &title, Family::Ui, BOLD, MARGIN, r.y + 50, ROW_PX);
            let mut second = if !entry.authors.is_empty() {
                entry.authors.join(", ")
            } else if entry.format == Format::Pdf {
                tr(&Msg::PdfDocument)
            } else {
                tr(&Msg::UnknownAuthor)
            };
            if !entry.readable {
                second = tr(&Msg::Unreadable);
            } else if let Some(&p) = self.progress.get(&entry.id).filter(|&&p| p > 0) {
                second = format!("{second} · {}", tr(&Msg::Percent(p)));
            }
            if entry.readable
                && let Some(note) = self.status_note(&entry.id)
            {
                second = format!("{second} · {}", tr(&note));
            }
            let second = one_line(t, &second, PLAIN, 30.0, max_w);
            t.draw_plain(canvas, &second, Family::Ui, PLAIN, MARGIN, r.y + 95, 30.0);
        }
    }

    pub(super) fn draw_message_lines(&self, canvas: &mut Canvas, msg: &Msg, top: i32) {
        let t = &self.text;
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
        for (i, line) in t
            .wrap_plain(&tr(msg), Family::Ui, PLAIN, ROW_PX, max_w)
            .iter()
            .enumerate()
        {
            t.draw_plain(
                canvas,
                line,
                Family::Ui,
                PLAIN,
                MARGIN,
                top + i as i32 * 52,
                ROW_PX,
            );
        }
    }

    // --- İçindekiler ---

    /// İçindekiler satır sayısı: kitabın listesi, yoksa her bölüm; PDF'te her sayfa.
    fn contents_len(&self) -> usize {
        self.session.as_ref().map_or(0, |s| {
            if s.pdf().is_some() || s.toc().is_empty() {
                s.chapter_count()
            } else {
                s.toc().len()
            }
        })
    }

    /// Satırın etiketi, girintisi ve açacağı bölüm (PDF'te sayfa).
    fn contents_row(&self, index: usize) -> (String, u32, usize) {
        let Some(s) = &self.session else {
            return (String::new(), 0, 0);
        };
        if s.pdf().is_some() {
            return (tr(&Msg::PageNumber(index + 1)), 0, index);
        }
        match s.toc().get(index) {
            Some(entry) => (entry.title.clone(), entry.depth, entry.spine_index),
            None => (tr(&Msg::ChapterNumber(index + 1)), 0, index),
        }
    }

    /// Okunan yerin içindekilerdeki satırı.
    fn current_contents_row(&self) -> Option<usize> {
        let s = self.session.as_ref().filter(|s| !s.pages.is_empty())?;
        Some(if s.pdf().is_some() || s.toc().is_empty() {
            s.spine
        } else {
            s.toc()
                .iter()
                .rposition(|t| t.spine_index <= s.spine)
                .unwrap_or(0)
        })
    }

    pub(super) fn contents_page_of_current(&self) -> usize {
        self.current_contents_row().unwrap_or(0) / CONTENTS.per_page()
    }

    pub(super) fn handle_contents(&mut self, page: usize, event: Event) -> Option<Refresh> {
        if let Event::Tap { x, y } = event {
            if back_button(self.width).contains(x, y) {
                return self.back_to_reading();
            }
            let index = CONTENTS.row_at(page, self.contents_len(), y)?;
            self.open_chapter(self.contents_row(index).2, Target::Start);
            return Some(self.refresh(true));
        }
        let page = Self::page_list(page, CONTENTS.pages(self.contents_len()), event)?;
        self.view = View::Contents { page };
        Some(self.refresh(false))
    }

    /// Okunan bir bölüm varsa okumaya, yoksa kütüphaneye döner.
    fn back_to_reading(&mut self) -> Option<Refresh> {
        let reading = self.session.as_ref().is_some_and(|s| !s.pages.is_empty());
        self.view = if reading {
            View::Reading
        } else {
            View::Library { page: 0 }
        };
        Some(self.refresh(true))
    }

    pub(super) fn draw_contents(&self, canvas: &mut Canvas, page: usize) {
        let Some(s) = &self.session else {
            return;
        };
        let t = &self.text;
        draw_header(t, canvas, &s.title(), &tr(&Msg::Contents), true);
        let current = self.current_contents_row();
        for (index, row) in CONTENTS.visible(page, self.contents_len()) {
            let (label, depth, _) = self.contents_row(index);
            let x = MARGIN + depth.min(4) as i32 * TOC_INDENT;
            let style = if current == Some(index) { BOLD } else { PLAIN };
            let text = one_line(
                t,
                &label,
                style,
                ROW_PX,
                (canvas.width() as i32 - MARGIN - x) as f32,
            );
            let r = CONTENTS.row_rect(canvas.width(), row);
            t.draw_plain(canvas, &text, Family::Ui, style, x, r.y + 46, ROW_PX);
        }
        let footer = Msg::ListFooter {
            page: page + 1,
            pages: CONTENTS.pages(self.contents_len()),
        };
        draw_footer(t, canvas, &footer, MARGIN);
    }

    // --- Okuma ---

    pub(super) fn handle_reading(&mut self, event: Event) -> Option<Refresh> {
        match event {
            // Görselde uzun basma tam ekran açar (dokunma sayfa çevirmeye kalır:
            // sayfayı kaplayan kapakta başka türlü çevrilemezdi).
            Event::Hold { x, y } if self.image_at(x, y).is_some() => {
                self.view = View::Image {
                    block: self.image_at(x, y)?,
                };
                return Some(self.refresh(true));
            }
            Event::Hold { x, y } => return self.start_selection(x, y),
            Event::HoldMove { x, y } => return self.extend_selection(x, y),
            Event::HoldEnd { .. } => return self.finish_selection(),
            Event::Tap { x, y } => {
                if self.link_back_button().is_some_and(|r| r.contains(x, y)) {
                    return self.go_back();
                }
                if self.results_button().is_some_and(|r| r.contains(x, y)) {
                    self.view = View::SearchResults { page: 0 };
                    return Some(self.refresh(true));
                }
                if let Some(index) = self.annotation_at(x, y) {
                    self.view = View::Annotation { index };
                    return Some(self.refresh(false));
                }
                if let Some((link, text)) = self.link_at(x, y)
                    && let Some(refresh) = self.open_link(&link, &text)
                {
                    return Some(refresh);
                }
            }
            _ => {}
        }
        if let Event::Pinch { permille } = event {
            return self.pinch(permille);
        }
        let forward = match event {
            Event::Key(PageKey::Forward) => true,
            Event::Key(PageKey::Back) => false,
            Event::Swipe { .. } => swipe_direction(event)?,
            Event::Tap { x, y } => match self.tap_zone(x, y) {
                Some(forward) => forward,
                None => {
                    // Menüden kütüphaneye ya da başka kitaba gidilebilir; konumu şimdi kaydet.
                    self.shutdown();
                    self.view = View::Menu;
                    return Some(self.refresh(false));
                }
            },
            _ => return None,
        };
        self.note_dwell(forward);
        let s = self.session.as_mut()?;
        // EPUB'da bölüm değişimi tam yenilenir; PDF'te her sayfa bir bölüm olduğundan normal sayaç.
        let full = s.pdf().is_none();
        // İki sütunda iki sütun birden çevrilir.
        let step = s.columns;
        if forward && s.page + step < s.pages.len() {
            s.page += step;
        } else if !forward && s.page >= step {
            s.page -= step;
        } else if forward && s.spine + spines(s) < s.chapter_count() {
            let next = s.spine + spines(s);
            self.open_chapter(next, Target::Start);
            return Some(self.refresh(full));
        } else if !forward && s.spine > 0 {
            let prev = s.spine.saturating_sub(spines(s));
            self.open_chapter(prev, Target::End);
            return Some(self.refresh(full));
        } else {
            return None;
        }
        self.page_turned();
        Some(self.refresh(false))
    }

    /// Okurken dokunulan yer: ileri (`Some(true)`), geri (`Some(false)`) ya da menü (`None`).
    pub(super) fn tap_zone(&self, x: u32, y: u32) -> Option<bool> {
        let (w, h) = (self.width, self.height);
        match self.settings.tap_zones {
            TapZones::Standard if x < w / 3 => Some(false),
            TapZones::Standard if x > 2 * w / 3 => Some(true),
            TapZones::Reversed if x < w / 3 => Some(true),
            TapZones::Reversed if x > 2 * w / 3 => Some(false),
            TapZones::Standard | TapZones::Reversed => None,
            TapZones::OneHand if y < h / 6 => None,
            TapZones::OneHand => Some(x >= w / 4),
        }
    }

    pub(super) fn draw_reading(&self, canvas: &mut Canvas) {
        let Some(s) = &self.session else {
            return;
        };
        let t = &self.text;
        let Some(page) = s.pages.get(s.page) else {
            return;
        };
        if s.showing_pdf_image() {
            self.draw_pdf_page(canvas, s);
            self.paint_marks(canvas);
            if s.bookmark_here().is_some() {
                draw_dog_ear(canvas);
            }
            let footer = if s.right.is_some() {
                Msg::PdfSpreadFooter {
                    page: s.spine + 1,
                    pages: s.chapter_count(),
                    percent: s.percent(),
                }
            } else if s.pages.len() > 1 {
                Msg::PdfScreenFooter {
                    page: s.spine + 1,
                    pages: s.chapter_count(),
                    screen: s.page + 1,
                    screens: s.pages.len(),
                    percent: s.percent(),
                }
            } else {
                Msg::PdfFooter {
                    page: s.spine + 1,
                    pages: s.chapter_count(),
                    percent: s.percent(),
                }
            };
            draw_footer(t, canvas, &self.with_time_left(footer), MARGIN);
            self.draw_link_back(canvas);
            self.draw_results_button(canvas);
            return;
        }
        for (index, left) in s.visible(self.settings.margin, self.width) {
            for placed in &s.pages[index].lines {
                if !self.draw_book_image(canvas, s, placed, left) {
                    t.draw_line(canvas, &placed.line, left, placed.baseline);
                }
            }
        }
        self.paint_marks(canvas);
        if page.lines.is_empty() {
            t.draw_msg(canvas, &Msg::EmptyChapter, MARGIN, TOP + 300, ROW_PX);
        }
        if s.bookmark_here().is_some() {
            draw_dog_ear(canvas);
        }
        let footer = if s.pdf().is_some() {
            Msg::PdfScreenFooter {
                page: s.spine + 1,
                pages: s.chapter_count(),
                screen: s.page + 1,
                screens: s.pages.len(),
                percent: s.percent(),
            }
        } else {
            Msg::ReadingFooter {
                chapter: s.spine + 1,
                chapters: s.chapter_count(),
                page: s.page + 1,
                pages: s.pages.len(),
                percent: s.percent(),
            }
        };
        draw_footer(
            t,
            canvas,
            &self.with_time_left(footer),
            self.settings.margin,
        );
        self.draw_link_back(canvas);
        self.draw_results_button(canvas);
    }

    // --- Menü ---

    /// Menü öğesinin düğmesi. Üst çubuk: kütüphane, kitap adı, yer imi; altında
    /// gezinme. Alt çubuk (alt bilgi satırının üstünde): ayarlar ve işlemler.
    pub(super) fn menu_rect(&self, index: usize) -> Rect {
        let items = self.menu_items();
        let w = self.width as i32;
        let slot = |row_y: i32, i: usize, n: usize| {
            let n = n.max(1) as i32;
            let bw = (w - 2 * MARGIN - (n - 1) * BAR_GAP) / n;
            Rect {
                x: MARGIN + i as i32 * (bw + BAR_GAP),
                y: row_y,
                w: bw,
                h: BAR_ROW,
            }
        };
        let row_of = |group: &[MenuItem]| {
            // Çubuktaki sıra grubun sırasıdır; olmayan öğe (EPUB'da aranabilir PDF) atlanır.
            let members: Vec<&MenuItem> = group.iter().filter(|g| items.contains(g)).collect();
            let i = members
                .iter()
                .position(|m| **m == items[index])
                .unwrap_or(0);
            (i, members.len())
        };
        match items.get(index) {
            Some(MenuItem::Library) => Rect {
                x: MARGIN,
                y: BAR_GAP,
                w: BAR_SIDE_W,
                h: BAR_ROW,
            },
            Some(MenuItem::Bookmark) => Rect {
                x: w - MARGIN - BAR_SIDE_W,
                y: BAR_GAP,
                w: BAR_SIDE_W,
                h: BAR_ROW,
            },
            Some(
                MenuItem::Contents
                | MenuItem::Search
                | MenuItem::Bookmarks
                | MenuItem::Annotations
                | MenuItem::Words,
            ) => {
                let (i, n) = row_of(&NAV_ITEMS);
                slot(2 * BAR_GAP + BAR_ROW, i, n)
            }
            _ => {
                let (i, n) = row_of(&ACTION_ITEMS);
                slot(bottom_of(self.height) - BAR_GAP - BAR_ROW, i, n)
            }
        }
    }

    /// Üst ve alt çubuğun kapladığı alanlar; dışına dokunmak menüyü kapatır.
    fn menu_bars(&self) -> (Rect, Rect) {
        let w = self.width as i32;
        let top = Rect {
            x: 0,
            y: 0,
            w,
            h: 3 * BAR_GAP + 2 * BAR_ROW,
        };
        let bottom = Rect {
            x: 0,
            y: bottom_of(self.height) - 2 * BAR_GAP - BAR_ROW,
            w,
            h: 2 * BAR_GAP + BAR_ROW,
        };
        (top, bottom)
    }

    fn menu_label(&self, item: MenuItem) -> Msg {
        match item {
            MenuItem::Contents => Msg::Contents,
            MenuItem::Search => Msg::Search,
            MenuItem::Bookmark
                if self
                    .session
                    .as_ref()
                    .is_some_and(|s| s.bookmark_here().is_some()) =>
            {
                Msg::RemoveBookmark
            }
            MenuItem::Bookmark => Msg::AddBookmark,
            MenuItem::Bookmarks => Msg::Bookmarks,
            MenuItem::Annotations => Msg::NotesShort,
            MenuItem::Words => Msg::Words,
            MenuItem::ExportNotes => Msg::ExportNotes,
            MenuItem::Settings => Msg::TextSettings,
            MenuItem::SaveSearchable => Msg::SaveSearchable,
            MenuItem::AutoTurn if self.auto_turn.is_some() => Msg::AutoTurnStop,
            MenuItem::AutoTurn => Msg::AutoTurnStart,
            MenuItem::Library => Msg::BackToLibrary,
        }
    }

    pub(super) fn handle_menu(&mut self, event: Event) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            // Sayfa tuşu menüyü kapatır.
            self.view = View::Reading;
            return Some(self.refresh(false));
        };
        let items = self.menu_items();
        let Some(item) = (0..items.len())
            .find(|&i| self.menu_rect(i).contains(x, y))
            .map(|i| items[i])
        else {
            let (top, bottom) = self.menu_bars();
            if top.contains(x, y) || bottom.contains(x, y) {
                return None; // çubukta boş yer
            }
            self.view = View::Reading;
            return Some(self.refresh(false));
        };
        match item {
            MenuItem::Contents => {
                self.view = View::Contents {
                    page: self.contents_page_of_current(),
                }
            }
            MenuItem::Bookmark => {
                if let Some(s) = self.session.as_mut() {
                    s.toggle_bookmark();
                }
                self.save_progress();
                self.view = View::Reading;
            }
            MenuItem::Bookmarks => self.view = View::Bookmarks { page: 0 },
            MenuItem::Annotations => self.view = View::Annotations { page: 0 },
            MenuItem::Words => self.view = View::Words { page: 0 },
            MenuItem::ExportNotes => self.show_export_result(),
            MenuItem::Settings => {
                self.view = View::Settings {
                    draft: self.settings.clone(),
                    page: 0,
                }
            }
            MenuItem::SaveSearchable => {
                let msg = self.save_searchable();
                self.view = View::Message {
                    msg,
                    back: Back::Reading,
                };
            }
            MenuItem::AutoTurn => {
                if self.auto_turn.is_some() {
                    self.stop_auto_turn();
                } else {
                    self.start_auto_turn();
                }
                self.view = View::Reading;
            }
            MenuItem::Search => {
                if self.search.is_some() {
                    self.view = View::SearchResults { page: 0 };
                } else {
                    return self.open_search_form();
                }
            }
            MenuItem::Library => {
                self.session = None;
                self.link_back.clear();
                self.search = None;
                self.search_mark = None;
                self.use_settings(self.defaults.clone());
                self.view = View::Library { page: 0 };
            }
        }
        Some(self.refresh(true))
    }

    pub(super) fn draw_menu(&self, canvas: &mut Canvas) {
        let t = &self.text;
        let (top, bottom) = self.menu_bars();
        canvas.fill_rect(top.x, top.y, top.w as u32, top.h as u32, 255);
        canvas.fill_rect(0, top.y + top.h - 3, top.w as u32, 3, 0);
        canvas.fill_rect(bottom.x, bottom.y, bottom.w as u32, bottom.h as u32, 255);
        canvas.fill_rect(0, bottom.y, bottom.w as u32, 3, 0);
        // Kitabın adı, yan düğmelerin arasında ortalanmış.
        if let Some(s) = &self.session {
            let max_w = (self.width as i32 - 2 * MARGIN - 2 * BAR_SIDE_W - 2 * BAR_GAP) as f32;
            let title = one_line(t, &s.title(), BOLD, ROW_PX, max_w);
            let tw = t.width_plain(&title, Family::Ui, BOLD, ROW_PX);
            let x = (self.width as f32 - tw) as i32 / 2;
            let baseline = BAR_GAP + BAR_ROW / 2 + (ROW_PX * 0.35) as i32;
            t.draw_plain(canvas, &title, Family::Ui, BOLD, x, baseline, ROW_PX);
        }
        for (i, item) in self.menu_items().iter().enumerate() {
            draw_button(
                t,
                canvas,
                self.menu_rect(i),
                &tr(&self.menu_label(*item)),
                BAR_PX,
            );
        }
    }

    // --- Yer imleri ---

    fn bookmark_count(&self) -> usize {
        self.session.as_ref().map_or(0, |s| s.state.bookmarks.len())
    }

    pub(super) fn handle_bookmarks(&mut self, page: usize, event: Event) -> Option<Refresh> {
        if let Event::Tap { x, y } = event {
            if back_button(self.width).contains(x, y) {
                return self.back_to_reading();
            }
            let index = BOOKMARKS.row_at(page, self.bookmark_count(), y)?;
            let b = self.session.as_ref()?.state.bookmarks[index].clone();
            self.open_chapter(b.spine, Target::At(b.position));
            return Some(self.refresh(true));
        }
        let page = Self::page_list(page, BOOKMARKS.pages(self.bookmark_count()), event)?;
        self.view = View::Bookmarks { page };
        Some(self.refresh(false))
    }

    pub(super) fn draw_bookmarks(&self, canvas: &mut Canvas, page: usize) {
        let Some(s) = &self.session else {
            return;
        };
        let t = &self.text;
        draw_header(t, canvas, &s.title(), &tr(&Msg::Bookmarks), true);
        if s.state.bookmarks.is_empty() {
            self.draw_message_lines(canvas, &Msg::NoBookmarks, HEADER_BOTTOM + 80);
        }
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
        for (index, row) in BOOKMARKS.visible(page, s.state.bookmarks.len()) {
            let b = &s.state.bookmarks[index];
            let r = BOOKMARKS.row_rect(canvas.width(), row);
            let snippet = one_line(t, &b.snippet, PLAIN, ROW_PX, max_w);
            t.draw_plain(
                canvas,
                &snippet,
                Family::Ui,
                PLAIN,
                MARGIN,
                r.y + 50,
                ROW_PX,
            );
            if s.pdf().is_none() {
                t.draw_msg(
                    canvas,
                    &Msg::ChapterNumber(b.spine + 1),
                    MARGIN,
                    r.y + 95,
                    30.0,
                );
            }
        }
        let footer = Msg::ListFooter {
            page: page + 1,
            pages: BOOKMARKS.pages(s.state.bookmarks.len()),
        };
        draw_footer(t, canvas, &footer, MARGIN);
    }

    // --- Vurgular ve notlar ---

    /// Listede görünen vurgular (`state.annotations` sırası), kitaptaki sırayla;
    /// arama varsa alıntısında, notunda ya da etiketinde geçenler.
    pub(super) fn annotation_rows(&self) -> Vec<usize> {
        let Some(s) = &self.session else {
            return Vec::new();
        };
        let query = self.annotation_query.as_deref().map(super::shelf::fold);
        s.annotations_in_order()
            .into_iter()
            .filter(|&i| {
                let a = &s.state.annotations[i];
                query.as_ref().is_none_or(|q| {
                    [Some(a.quote.as_str()), a.note.as_deref(), a.tag.as_deref()]
                        .into_iter()
                        .flatten()
                        .any(|t| super::shelf::fold(t).contains(q.as_str()))
                })
            })
            .collect()
    }

    /// Vurgular ekranının başlığındaki "Ara" / "× Arama".
    pub(super) fn annotation_search_button(&self) -> Rect {
        header_button(self.width, 200)
    }

    pub(super) fn handle_annotations(&mut self, page: usize, event: Event) -> Option<Refresh> {
        if let Event::Tap { x, y } = event {
            if back_button(self.width).contains(x, y) {
                self.annotation_query = None;
                return self.back_to_reading();
            }
            if self.annotation_search_button().contains(x, y) {
                if self.annotation_query.take().is_some() {
                    self.view = View::Annotations { page: 0 };
                    return Some(self.refresh(false));
                }
                let field = super::form::Field::new(Msg::FieldQuery, "");
                let form =
                    super::form::Form::new(super::form::FormKind::AnnotationSearch, vec![field]);
                return self.open_form(form);
            }
            let rows = self.annotation_rows();
            let row = ANNOTATIONS.row_at(page, rows.len(), y)?;
            let s = self.session.as_ref()?;
            let a = &s.state.annotations[rows[row]];
            let (spine, at) = (a.spine, a.start);
            self.open_chapter(spine, Target::At(at));
            return Some(self.refresh(true));
        }
        let page = Self::page_list(page, ANNOTATIONS.pages(self.annotation_rows().len()), event)?;
        self.view = View::Annotations { page };
        Some(self.refresh(false))
    }

    pub(super) fn draw_annotations(&self, canvas: &mut Canvas, page: usize) {
        let Some(s) = &self.session else {
            return;
        };
        let t = &self.text;
        let order = self.annotation_rows();
        let subtitle = match &self.annotation_query {
            Some(q) => format!(
                "{} · {}",
                tr(&Msg::Annotations),
                tr(&Msg::SearchTitle(q.clone()))
            ),
            None => tr(&Msg::Annotations),
        };
        let search = self.annotation_search_button();
        draw_header_until(t, canvas, &s.title(), &subtitle, search.x - 30);
        if !s.state.annotations.is_empty() {
            let label = if self.annotation_query.is_some() {
                Msg::ClearLibrarySearch
            } else {
                Msg::Search
            };
            draw_button(t, canvas, search, &tr(&label), ROW_PX);
        }
        draw_button(
            t,
            canvas,
            back_button(canvas.width()),
            &tr(&Msg::Back),
            ROW_PX,
        );
        if s.state.annotations.is_empty() {
            self.draw_message_lines(canvas, &Msg::NoAnnotations, HEADER_BOTTOM + 80);
        } else if order.is_empty() {
            self.draw_message_lines(canvas, &Msg::NoSearchHits, HEADER_BOTTOM + 80);
        }
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
        for (index, row) in ANNOTATIONS.visible(page, order.len()) {
            let a = &s.state.annotations[order[index]];
            let r = ANNOTATIONS.row_rect(canvas.width(), row);
            let quote = one_line(t, &a.quote.replace('\n', " "), PLAIN, ROW_PX, max_w);
            t.draw_plain(canvas, &quote, Family::Ui, PLAIN, MARGIN, r.y + 50, ROW_PX);
            let mut second = s.chapter_title(a.spine);
            if let Some(tag) = &a.tag {
                second = format!("{second} · {}", tr(&Msg::TagLine(tag.clone())));
            }
            if let Some(note) = &a.note {
                second = format!("{second} · {}", tr(&Msg::NoteLine(note.clone())));
            }
            let second = one_line(t, &second, PLAIN, 30.0, max_w);
            t.draw_plain(canvas, &second, Family::Ui, PLAIN, MARGIN, r.y + 95, 30.0);
        }
        let footer = Msg::ListFooter {
            page: page + 1,
            pages: ANNOTATIONS.pages(order.len()),
        };
        draw_footer(t, canvas, &footer, MARGIN);
    }

    // --- Yazı ayarları ---

    /// Ayar satırının azaltma ve artırma düğmeleri.
    pub(super) fn settings_buttons(&self, row: usize) -> (Rect, Rect) {
        let y = HEADER_BOTTOM + row as i32 * SETTINGS_ROW + (SETTINGS_ROW - STEP_BUTTON) / 2;
        let right = self.width as i32 - MARGIN;
        let plus = Rect {
            x: right - STEP_BUTTON,
            y,
            w: STEP_BUTTON,
            h: STEP_BUTTON,
        };
        let minus = Rect {
            x: right - 2 * STEP_BUTTON - 20,
            y,
            w: STEP_BUTTON,
            h: STEP_BUTTON,
        };
        (minus, plus)
    }

    pub(super) fn settings_ok(&self) -> Rect {
        Rect {
            x: self.width as i32 / 2 + 20,
            y: BOTTOM - 100,
            w: self.width as i32 / 2 - 20 - MARGIN,
            h: 100,
        }
    }

    pub(super) fn settings_cancel(&self) -> Rect {
        Rect {
            x: MARGIN,
            y: BOTTOM - 100,
            w: self.width as i32 / 2 - 20 - MARGIN,
            h: 100,
        }
    }

    /// Yazı ve PDF ayarları sayfaları arasında geçiş düğmesi.
    pub(super) fn settings_page_button(&self) -> Rect {
        Rect {
            x: MARGIN,
            y: BOTTOM - 220,
            w: self.width as i32 - 2 * MARGIN,
            h: 90,
        }
    }

    pub(super) fn handle_settings(
        &mut self,
        draft: Settings,
        page: usize,
        event: Event,
    ) -> Option<Refresh> {
        let other = (page + 1) % FIELD_PAGES.len();
        let other = if matches!(event, Event::Key(PageKey::Back)) {
            (page + FIELD_PAGES.len() - 1) % FIELD_PAGES.len()
        } else {
            other
        };
        let Event::Tap { x, y } = event else {
            // Sayfa tuşları ayar sayfasını değiştirir.
            self.view = View::Settings { draft, page: other };
            return Some(self.refresh(false));
        };
        if self.settings_page_button().contains(x, y) {
            self.view = View::Settings { draft, page: other };
            return Some(self.refresh(false));
        }
        if self.settings_ok().contains(x, y) {
            self.apply_settings(draft);
            return self.back_to_reading();
        }
        if self.session.is_some() && self.settings_default_button().contains(x, y) {
            self.make_default(draft);
            return self.back_to_reading();
        }
        if self.settings_cancel().contains(x, y) {
            return self.back_to_reading();
        }
        for (row, field) in FIELD_PAGES[page].iter().enumerate() {
            let (minus, plus) = self.settings_buttons(row);
            let up = if plus.contains(x, y) {
                true
            } else if minus.contains(x, y) {
                false
            } else {
                continue;
            };
            self.view = View::Settings {
                draft: draft.step(*field, up),
                page,
            };
            return Some(self.refresh(false));
        }
        None
    }

    /// Başlıkta "Varsayılan yap": bu ayarlar bütün kitaplara.
    pub(super) fn settings_default_button(&self) -> Rect {
        Rect {
            x: self.width as i32 - MARGIN - 300,
            y: TOP,
            w: 300,
            h: 72,
        }
    }

    pub(super) fn draw_settings(&self, canvas: &mut Canvas, draft: &Settings, page: usize) {
        let t = &self.text;
        if self.session.is_some() {
            // Kitap açık: yazı ayarları yalnız bu kitaba uygulanır.
            let make_default = self.settings_default_button();
            draw_header_until(
                t,
                canvas,
                &tr(&PAGE_TITLES[page]),
                &tr(&Msg::SettingsHintBook),
                make_default.x - 30,
            );
            draw_button(t, canvas, make_default, &tr(&Msg::MakeDefault), ROW_PX);
        } else {
            draw_header(
                t,
                canvas,
                &tr(&PAGE_TITLES[page]),
                &tr(&Msg::SettingsHint),
                false,
            );
        }
        for (row, field) in FIELD_PAGES[page].iter().enumerate() {
            let top = HEADER_BOTTOM + row as i32 * SETTINGS_ROW;
            t.draw_msg(canvas, &field.label(), MARGIN, top + 52, ROW_PX);
            t.draw_plain(
                canvas,
                &draft.value_text(*field),
                Family::Ui,
                BOLD,
                MARGIN,
                top + 100,
                ROW_PX,
            );
            let (minus, plus) = self.settings_buttons(row);
            draw_button(t, canvas, minus, "−", 52.0);
            draw_button(t, canvas, plus, "+", 52.0);
        }
        let next = Msg::NextSettingsPage(Box::new(
            PAGE_TITLES[(page + 1) % FIELD_PAGES.len()].clone(),
        ));
        draw_button(t, canvas, self.settings_page_button(), &tr(&next), 40.0);
        draw_button(t, canvas, self.settings_cancel(), &tr(&Msg::Cancel), 40.0);
        draw_button(t, canvas, self.settings_ok(), &tr(&Msg::Ok), 40.0);
        if page != 0 {
            return;
        }
        // Önizleme: seçilen yazı tipi ve boyutuyla örnek cümle.
        let preview_y =
            HEADER_BOTTOM + FIELD_PAGES[0].len() as i32 * SETTINGS_ROW + 40 + draft.font_px as i32;
        let book = Family::Book(draft.family);
        let max_w = (canvas.width() as i32 - 2 * draft.margin) as f32;
        let sample = t.wrap_plain(
            &tr(&Msg::PreviewSample),
            book,
            Style::default(),
            draft.font_px,
            max_w,
        );
        if let Some(line) = sample.first() {
            // Önizleme taslağın kalınlığıyla; sonra uygulanmış ayara dönülür.
            t.set_weight(draft.weight);
            t.draw_plain(
                canvas,
                line,
                book,
                Style::default(),
                draft.margin,
                preview_y,
                draft.font_px,
            );
            t.set_weight(self.settings.weight);
        }
    }
}

/// Bir çevirmede geçilen bölüm sayısı: yatayda yan yana iki PDF sayfası 2.
fn spines(s: &super::session::Session) -> usize {
    if s.spread { 2 } else { 1 }
}

/// Sayfa çeviren kaydırmanın en kısa yatay yolu (piksel; 300 ppi'de ~1,3 cm).
const SWIPE_MIN: i32 = 150;
/// İki parmağın puntoyu bir adım değiştirmesi için en az açılma/kapanma.
const PINCH_IN: u32 = 870;
const PINCH_OUT: u32 = 1150;

/// Yatay kaydırma: sola kaydırma ileri (`true`), sağa geri. Kısa ya da dikey kaydırma `None`.
fn swipe_direction(event: Event) -> Option<bool> {
    let Event::Swipe { dx, dy } = event else {
        return None;
    };
    (dx.abs() >= SWIPE_MIN && dx.abs() > 2 * dy.abs()).then_some(dx < 0)
}

impl App {
    /// İki parmak: açılınca punto (PDF görüntüsünde yakınlaştırma) bir adım
    /// büyür, kapanınca küçülür. Kitaba özel ayar olarak uygulanır.
    fn pinch(&mut self, permille: u32) -> Option<Refresh> {
        let up = match permille {
            p if p >= PINCH_OUT => true,
            p if p <= PINCH_IN => false,
            _ => return None,
        };
        let image = self.session.as_ref()?.showing_pdf_image();
        let field = if image { Field::PdfZoom } else { Field::Size };
        let next = self.settings.step(field, up);
        if next == self.settings {
            return None;
        }
        self.apply_settings(next);
        Some(self.refresh(true))
    }
}

/// Yer imli sayfanın sağ üst köşesinde kıvrık köşe işareti.
fn draw_dog_ear(canvas: &mut Canvas) {
    let size = 56;
    let right = canvas.width() as i32;
    for row in 0..size {
        canvas.fill_rect(right - size + row, row, (size - row) as u32, 1, 0);
    }
}
