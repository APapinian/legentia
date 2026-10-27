//! Ekranlar: her biri için olay işleme ve çizim.

use super::session::Target;
use super::settings::{FIELD_PAGES, Settings};
use super::ui::{
    BOLD, BOTTOM, HEADER_BOTTOM, List, MARGIN, PLAIN, ROW_PX, Rect, TOP, back_button, draw_button,
    draw_footer, draw_header, one_line,
};
use super::{App, Back, View};
use crate::canvas::Canvas;
use crate::event::{Event, PageKey, Refresh};
use crate::i18n::{Msg, tr};
use crate::library::Format;
use crate::text::{Family, Style};

pub(super) const LIBRARY: List = List {
    top: HEADER_BOTTOM,
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
const TOC_INDENT: i32 = 40;
const MENU_ROW: i32 = 100;
const SETTINGS_ROW: i32 = 128;
const STEP_BUTTON: i32 = 100;

/// Okurken açılan menünün seçenekleri.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MenuItem {
    Contents,
    Bookmark,
    Bookmarks,
    Settings,
    SaveSearchable,
    Library,
    Close,
}

impl App {
    /// Menünün seçenekleri; "Aranabilir PDF olarak kaydet" yalnızca PDF'te.
    pub(super) fn menu_items(&self) -> Vec<MenuItem> {
        let mut items = vec![
            MenuItem::Contents,
            MenuItem::Bookmark,
            MenuItem::Bookmarks,
            MenuItem::Settings,
        ];
        if self.session.as_ref().is_some_and(|s| s.pdf().is_some()) {
            items.push(MenuItem::SaveSearchable);
        }
        items.extend([MenuItem::Library, MenuItem::Close]);
        items
    }

    // --- Ortak ---

    /// Liste ekranlarında sayfa tuşları: listeyi çevirir; uçta `None`.
    fn page_list(page: usize, pages: usize, event: Event) -> Option<usize> {
        match event {
            Event::Key(PageKey::Forward) if page + 1 < pages => Some(page + 1),
            Event::Key(PageKey::Back) if page > 0 => Some(page - 1),
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
        if let Event::Tap { y, .. } = event {
            let index = LIBRARY.row_at(page, self.library.len(), y)?;
            let path = self.library[index].path.clone();
            self.open_book(&path);
            return Some(self.refresh(true));
        }
        let page = Self::page_list(page, LIBRARY.pages(self.library.len()), event)?;
        self.view = View::Library { page };
        Some(self.refresh(false))
    }

    pub(super) fn draw_library(&self, canvas: &mut Canvas, page: usize) {
        let t = &self.text;
        draw_header(
            t,
            canvas,
            &tr(&Msg::Library),
            &tr(&Msg::BookCount(self.library.len())),
            false,
        );
        if self.library.is_empty() {
            self.draw_message_lines(canvas, &Msg::LibraryEmpty, HEADER_BOTTOM + 80);
        }
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
        for (index, row) in LIBRARY.visible(page, self.library.len()) {
            let entry = &self.library[index];
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
            let second = one_line(t, &second, PLAIN, 30.0, max_w);
            t.draw_plain(canvas, &second, Family::Ui, PLAIN, MARGIN, r.y + 95, 30.0);
        }
        let footer = Msg::ListFooter {
            page: page + 1,
            pages: LIBRARY.pages(self.library.len()),
        };
        draw_footer(t, canvas, &footer, MARGIN);
    }

    fn draw_message_lines(&self, canvas: &mut Canvas, msg: &Msg, top: i32) {
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
        let third = self.width / 3;
        let forward = match event {
            Event::Key(PageKey::Forward) => true,
            Event::Key(PageKey::Back) => false,
            Event::Tap { x, .. } if x < third => false,
            Event::Tap { x, .. } if x > 2 * third => true,
            Event::Tap { .. } => {
                // Menüden kütüphaneye ya da başka kitaba gidilebilir; konumu şimdi kaydet.
                self.shutdown();
                self.view = View::Menu;
                return Some(self.refresh(false));
            }
        };
        let s = self.session.as_mut()?;
        // EPUB'da bölüm değişimi tam yenilenir; PDF'te her sayfa bir bölüm olduğundan normal sayaç.
        let full = s.pdf().is_none();
        if forward && s.page + 1 < s.pages.len() {
            s.page += 1;
        } else if !forward && s.page > 0 {
            s.page -= 1;
        } else if forward && s.spine + 1 < s.chapter_count() {
            let next = s.spine + 1;
            self.open_chapter(next, Target::Start);
            return Some(self.refresh(full));
        } else if !forward && s.spine > 0 {
            let prev = s.spine - 1;
            self.open_chapter(prev, Target::End);
            return Some(self.refresh(full));
        } else {
            return None;
        }
        self.page_turned();
        Some(self.refresh(false))
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
            if s.bookmark_here().is_some() {
                draw_dog_ear(canvas);
            }
            let footer = Msg::PdfFooter {
                page: s.spine + 1,
                pages: s.chapter_count(),
                percent: s.percent(),
            };
            draw_footer(t, canvas, &footer, MARGIN);
            return;
        }
        for placed in &page.lines {
            t.draw_line(canvas, &placed.line, self.settings.margin, placed.baseline);
        }
        if page.lines.is_empty() {
            t.draw_msg(canvas, &Msg::EmptyChapter, MARGIN, TOP + 300, ROW_PX);
        }
        if s.bookmark_here().is_some() {
            draw_dog_ear(canvas);
        }
        let footer = if s.pdf().is_some() {
            Msg::PdfReflowFooter {
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
        draw_footer(t, canvas, &footer, self.settings.margin);
    }

    // --- Menü ---

    pub(super) fn menu_rect(&self, index: usize) -> Rect {
        Rect {
            x: MARGIN,
            y: TOP + index as i32 * MENU_ROW,
            w: self.width as i32 - 2 * MARGIN,
            h: MENU_ROW,
        }
    }

    fn menu_label(&self, item: MenuItem) -> Msg {
        match item {
            MenuItem::Contents => Msg::Contents,
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
            MenuItem::Settings => Msg::TextSettings,
            MenuItem::SaveSearchable => Msg::SaveSearchable,
            MenuItem::Library => Msg::Library,
            MenuItem::Close => Msg::BackToReading,
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
            MenuItem::Library => {
                self.session = None;
                self.view = View::Library { page: 0 };
            }
            MenuItem::Close => self.view = View::Reading,
        }
        Some(self.refresh(true))
    }

    pub(super) fn draw_menu(&self, canvas: &mut Canvas) {
        let t = &self.text;
        let whole = Rect {
            x: MARGIN,
            y: TOP,
            w: self.width as i32 - 2 * MARGIN,
            h: self.menu_items().len() as i32 * MENU_ROW,
        };
        canvas.fill_rect(
            whole.x - 10,
            whole.y - 10,
            (whole.w + 20) as u32,
            (whole.h + 20) as u32,
            255,
        );
        canvas.stroke_rect(whole.x, whole.y, whole.w as u32, whole.h as u32, 3);
        for (i, item) in self.menu_items().iter().enumerate() {
            let r = self.menu_rect(i);
            if i > 0 {
                canvas.fill_rect(r.x, r.y, r.w as u32, 2, 0);
            }
            t.draw_plain(
                canvas,
                &tr(&self.menu_label(*item)),
                Family::Ui,
                PLAIN,
                r.x + 40,
                r.y + 62,
                40.0,
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

    pub(super) fn draw_settings(&self, canvas: &mut Canvas, draft: &Settings, page: usize) {
        let t = &self.text;
        let title = if page == 0 {
            Msg::TextSettings
        } else {
            Msg::PdfSettings
        };
        draw_header(t, canvas, &tr(&title), &tr(&Msg::SettingsHint), false);
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
        let other = if page == 0 {
            Msg::PdfSettingsNext
        } else {
            Msg::TextSettingsBack
        };
        draw_button(t, canvas, self.settings_page_button(), &tr(&other), 40.0);
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
            t.draw_plain(
                canvas,
                line,
                book,
                Style::default(),
                draft.margin,
                preview_y,
                draft.font_px,
            );
        }
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
