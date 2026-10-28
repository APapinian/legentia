//! Kütüphane araç çubuğu (başlığın altında): arama, sıralama ve okuma
//! durumu süzgeci. Kitaba uzun basınca okuma durumu seçilir. Durum elle
//! seçilmediyse ilerlemeden çıkar: hiç açılmamış "okunacak", açılmış
//! "okunuyor", %100 "bitti".

use serde::{Deserialize, Serialize};

use super::form::{Field, Form, FormKind};
use super::screens::LIBRARY;
use super::ui::{HEADER_BOTTOM, MARGIN, ROW_PX, Rect, draw_button, draw_header};
use super::{App, View, book_file};
use crate::canvas::Canvas;
use crate::collate;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};

/// Araç çubuğunun yüksekliği; liste altından başlar.
pub(super) const TOOLBAR_H: i32 = 100;
const TOOL_H: i32 = 80;
const TOOL_GAP: i32 = 16;
const SEARCH_W: i32 = 170;
const SORT_W: i32 = 290;
const TOOL_PX: f32 = 28.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ReadStatus {
    #[default]
    ToRead,
    Reading,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Sort {
    /// Son açılan önce; hiç açılmamışlar sonda, başlığa göre.
    #[default]
    Recent,
    Title,
    Author,
    /// Kütüphaneye en son eklenen (dosya tarihi) önce.
    Added,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Filter {
    #[default]
    All,
    Reading,
    ToRead,
    Finished,
}

const FILTERS: [Filter; 4] = [
    Filter::All,
    Filter::Reading,
    Filter::ToRead,
    Filter::Finished,
];
const STATUSES: [ReadStatus; 3] = [
    ReadStatus::ToRead,
    ReadStatus::Reading,
    ReadStatus::Finished,
];

/// Kütüphane listesindeki kitabın kayıttan gelen bilgisi (yüzde `progress`'te).
#[derive(Debug, Clone, Default)]
pub(super) struct Shelved {
    /// Son açılma (Unix saniyesi; 0 = hiç açılmadı).
    pub opened: u64,
    pub status: Option<ReadStatus>,
}

/// Arama için katlama: Türkçe küçük harf, şapkasız, noktalı/noktasız i aynı.
fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match collate::turkish_lower(c) {
            'â' => 'a',
            'î' | 'ı' => 'i',
            'û' => 'u',
            c => c,
        })
        .collect()
}

impl App {
    pub(super) fn status_of(&self, id: &str) -> ReadStatus {
        let info = self.shelf.get(id).cloned().unwrap_or_default();
        let percent = self.progress.get(id).copied().unwrap_or(0);
        info.status.unwrap_or(if percent >= 100 {
            ReadStatus::Finished
        } else if info.opened > 0 || percent > 0 {
            ReadStatus::Reading
        } else {
            ReadStatus::ToRead
        })
    }

    /// Kütüphanede görünen kitaplar (`library` sırası): arama, süzgeç, sıralama uygulanmış.
    pub(super) fn visible_books(&self) -> Vec<usize> {
        let query = self.library_query.as_deref().map(fold);
        let filter = self.settings.library_filter;
        let mut out: Vec<usize> = (0..self.library.len())
            .filter(|&i| {
                let e = &self.library[i];
                let matches = query.as_ref().is_none_or(|q| {
                    fold(&e.display_title()).contains(q.as_str())
                        || e.authors.iter().any(|a| fold(a).contains(q.as_str()))
                });
                let status = self.status_of(&e.id);
                let kept = match filter {
                    Filter::All => true,
                    Filter::Reading => status == ReadStatus::Reading,
                    Filter::ToRead => status == ReadStatus::ToRead,
                    Filter::Finished => status == ReadStatus::Finished,
                };
                matches && kept
            })
            .collect();
        let title = |i: &usize| self.library[*i].display_title();
        let opened = |i: &usize| self.shelf.get(&self.library[*i].id).map_or(0, |s| s.opened);
        match self.settings.library_sort {
            // Tarama zaten başlığa göre (Türkçe) sıralı; sıralamalar kararlı.
            Sort::Title => {}
            Sort::Recent => out.sort_by_key(|i| std::cmp::Reverse(opened(i))),
            Sort::Added => out.sort_by_key(|i| std::cmp::Reverse(self.library[*i].modified)),
            Sort::Author => out.sort_by(|a, b| {
                let author = |i: &usize| {
                    self.library[*i]
                        .authors
                        .first()
                        .cloned()
                        .unwrap_or_default()
                };
                collate::compare(&author(a), &author(b))
                    .then_with(|| collate::compare(&title(a), &title(b)))
            }),
        }
        out
    }

    pub(super) fn tool_rects(&self) -> (Rect, Rect, Vec<Rect>) {
        let y = HEADER_BOTTOM + (TOOLBAR_H - TOOL_H) / 2 - 10;
        let r = |x: i32, w: i32| Rect { x, y, w, h: TOOL_H };
        let search = r(MARGIN, SEARCH_W);
        let sort = r(search.x + SEARCH_W + TOOL_GAP, SORT_W);
        let left = sort.x + SORT_W + TOOL_GAP;
        let n = FILTERS.len() as i32;
        let fw = (self.width as i32 - MARGIN - left - (n - 1) * TOOL_GAP / 2) / n;
        let filters = (0..n)
            .map(|i| r(left + i * (fw + TOOL_GAP / 2), fw))
            .collect();
        (search, sort, filters)
    }

    /// Araç çubuğuna dokunma; dokunulmadıysa `None`.
    pub(super) fn handle_toolbar(&mut self, x: u32, y: u32) -> Option<Refresh> {
        let (search, sort, filters) = self.tool_rects();
        if search.contains(x, y) {
            if self.library_query.take().is_some() {
                self.view = View::Library { page: 0 };
                return Some(self.refresh(false));
            }
            let form = Form::new(
                FormKind::LibrarySearch,
                vec![Field::new(Msg::FieldQuery, "")],
            );
            return self.open_form(form);
        }
        let mut next = self.settings.clone();
        if sort.contains(x, y) {
            next.library_sort = match next.library_sort {
                Sort::Recent => Sort::Title,
                Sort::Title => Sort::Author,
                Sort::Author => Sort::Added,
                Sort::Added => Sort::Recent,
            };
        } else {
            let i = filters.iter().position(|r| r.contains(x, y))?;
            next.library_filter = FILTERS[i];
        }
        self.apply_settings(next);
        self.view = View::Library { page: 0 };
        Some(self.refresh(false))
    }

    pub(super) fn draw_toolbar(&self, canvas: &mut Canvas) {
        let t = &self.text;
        let (search, sort, filters) = self.tool_rects();
        let search_label = if self.library_query.is_some() {
            Msg::ClearLibrarySearch
        } else {
            Msg::Search
        };
        draw_button(t, canvas, search, &tr(&search_label), TOOL_PX);
        let sort_label = Msg::SortBy(match self.settings.library_sort {
            Sort::Recent => Box::new(Msg::SortRecent),
            Sort::Title => Box::new(Msg::SortTitle),
            Sort::Author => Box::new(Msg::SortAuthor),
            Sort::Added => Box::new(Msg::SortAdded),
        });
        draw_button(t, canvas, sort, &tr(&sort_label), TOOL_PX);
        for (f, r) in FILTERS.iter().zip(&filters) {
            let label = match f {
                Filter::All => Msg::FilterAll,
                Filter::Reading => Msg::StatusReading,
                Filter::ToRead => Msg::StatusToRead,
                Filter::Finished => Msg::StatusFinished,
            };
            draw_button(t, canvas, *r, &tr(&label), TOOL_PX);
            if *f == self.settings.library_filter {
                // Seçili süzgeç koyu çerçeveli.
                canvas.stroke_rect(r.x + 5, r.y + 5, (r.w - 10) as u32, (r.h - 10) as u32, 4);
            }
        }
    }

    // --- Okuma durumu (kitaba uzun basınca) ---

    pub(super) fn status_rect(&self, i: usize) -> Rect {
        Rect {
            x: MARGIN,
            y: HEADER_BOTTOM + 60 + i as i32 * 130,
            w: self.width as i32 - 2 * MARGIN,
            h: 110,
        }
    }

    pub(super) fn open_book_actions(&mut self, page: usize, y: u32) -> Option<Refresh> {
        let books = self.visible_books();
        let row = LIBRARY.row_at(page, books.len(), y)?;
        self.view = View::BookActions { book: books[row] };
        Some(self.refresh(true))
    }

    /// Kitabın okuma durumunu kaydeder (kitap açık olmasa da).
    pub(super) fn set_status(&mut self, index: usize, status: ReadStatus) {
        let Some(id) = self.library.get(index).map(|e| e.id.clone()) else {
            return;
        };
        let file = book_file(&id);
        let mut state: super::BookState = self.store.load(&file).unwrap_or_default();
        state.status = Some(status);
        self.store.save(&file, &state);
        if let Some(s) = self.session.as_mut().filter(|s| s.id == id) {
            s.state.status = Some(status);
        }
        self.shelf.entry(id).or_default().status = Some(status);
    }

    pub(super) fn handle_book_actions(&mut self, book: usize, event: Event) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        if let Some(i) = (0..STATUSES.len()).find(|&i| self.status_rect(i).contains(x, y)) {
            self.set_status(book, STATUSES[i]);
        }
        self.view = View::Library { page: 0 };
        Some(self.refresh(true))
    }

    pub(super) fn draw_book_actions(&self, canvas: &mut Canvas, book: usize) {
        let t = &self.text;
        let Some(e) = self.library.get(book) else {
            return;
        };
        draw_header(
            t,
            canvas,
            &e.display_title(),
            &tr(&Msg::ChooseStatus),
            false,
        );
        let current = self.status_of(&e.id);
        for (i, s) in STATUSES.iter().enumerate() {
            let r = self.status_rect(i);
            let label = match s {
                ReadStatus::ToRead => Msg::MarkToRead,
                ReadStatus::Reading => Msg::MarkReading,
                ReadStatus::Finished => Msg::MarkFinished,
            };
            draw_button(t, canvas, r, &tr(&label), ROW_PX);
            if *s == current {
                canvas.stroke_rect(r.x + 6, r.y + 6, (r.w - 12) as u32, (r.h - 12) as u32, 4);
            }
        }
        let hint_y = self.status_rect(STATUSES.len()).y + 50;
        t.draw_msg(canvas, &Msg::TapElsewhereToClose, MARGIN, hint_y, 30.0);
    }

    /// Satırın ikinci satırına eklenecek durum ("· bitti", "· okunacak").
    pub(super) fn status_note(&self, id: &str) -> Option<Msg> {
        match self.status_of(id) {
            ReadStatus::Finished => Some(Msg::StatusFinishedShort),
            ReadStatus::ToRead if self.shelf.get(id).is_some_and(|s| s.status.is_some()) => {
                Some(Msg::StatusToReadShort)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_folding_ignores_case_and_dots() {
        assert_eq!(fold("İstanbul ISPARTA Kâğıt"), "istanbul isparta kağit");
    }
}
