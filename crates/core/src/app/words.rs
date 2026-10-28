//! Kişisel kelime listesi: sözlükte bakılan kelimeler, sözlükteki biçimleri,
//! bulundukları kitap ve eklenme zamanıyla (`words.json`).

use serde::{Deserialize, Serialize};

use super::screens::WORDS;
use super::ui::{
    MARGIN, PLAIN, ROW_PX, Rect, back_button, draw_button, draw_footer, draw_header, one_line,
};
use super::{App, View};
use crate::LIBRA2_HEIGHT;
use crate::canvas::Canvas;
use crate::collate;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};
use crate::text::Family;

pub(super) const WORDS_FILE: &str = "words.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedWord {
    /// Sözlükte bulunan biçim (kök).
    pub word: String,
    /// Metinde geçtiği biçim.
    pub seen: String,
    /// Kelimenin geçtiği kitabın başlığı.
    pub book: String,
    /// Eklenme zamanı (Unix saniyesi).
    pub created: u64,
}

impl App {
    pub(super) fn is_saved(&self, word: &str) -> bool {
        let k = crate::dict::key(word);
        self.words.iter().any(|w| crate::dict::key(&w.word) == k)
    }

    /// Kelime listedeyse çıkarır, değilse ekler; liste Türk alfabesine göre sıralı kalır.
    pub(super) fn toggle_saved(&mut self, word: &str, seen: &str) {
        let k = crate::dict::key(word);
        if let Some(i) = self
            .words
            .iter()
            .position(|w| crate::dict::key(&w.word) == k)
        {
            self.words.remove(i);
        } else {
            let book = self.session.as_ref().map(|s| s.title()).unwrap_or_default();
            self.words.push(SavedWord {
                word: word.to_string(),
                seen: seen.to_string(),
                book,
                created: super::annotations::now(),
            });
            self.words
                .sort_by(|a, b| collate::compare(&a.word, &b.word));
        }
        self.store.save(WORDS_FILE, &self.words);
    }

    /// Tanım ekranındaki "Listeye ekle / Listeden çıkar" düğmesi (alt bilgi satırının sağında).
    pub(super) fn word_button(&self) -> Rect {
        Rect {
            x: self.width as i32 - MARGIN - 330,
            y: LIBRA2_HEIGHT as i32 - 100,
            w: 330,
            h: 80,
        }
    }

    pub(super) fn handle_words(&mut self, page: usize, event: Event) -> Option<Refresh> {
        if let Event::Tap { x, y } = event {
            if back_button(self.width).contains(x, y) {
                self.view = View::Reading;
                return Some(self.refresh(true));
            }
            let index = WORDS.row_at(page, self.words.len(), y)?;
            let word = self.words[index].word.clone();
            return self.look_up(&word);
        }
        let page = Self::page_list(page, WORDS.pages(self.words.len()), event)?;
        self.view = View::Words { page };
        Some(self.refresh(false))
    }

    pub(super) fn draw_words(&self, canvas: &mut Canvas, page: usize) {
        let t = &self.text;
        draw_header(
            t,
            canvas,
            &tr(&Msg::Words),
            &tr(&Msg::WordCount(self.words.len())),
            true,
        );
        if self.words.is_empty() {
            self.draw_message_lines(canvas, &Msg::NoWords, super::ui::HEADER_BOTTOM + 80);
        }
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
        for (index, row) in WORDS.visible(page, self.words.len()) {
            let w = &self.words[index];
            let r = WORDS.row_rect(canvas.width(), row);
            t.draw_plain(
                canvas,
                &w.word,
                Family::Ui,
                super::ui::BOLD,
                MARGIN,
                r.y + 50,
                ROW_PX,
            );
            let second =
                if w.seen.is_empty() || crate::dict::key(&w.seen) == crate::dict::key(&w.word) {
                    w.book.clone()
                } else {
                    format!("“{}” · {}", w.seen, w.book)
                };
            let second = one_line(t, &second, PLAIN, 30.0, max_w);
            t.draw_plain(canvas, &second, Family::Ui, PLAIN, MARGIN, r.y + 95, 30.0);
        }
        let footer = Msg::ListFooter {
            page: page + 1,
            pages: WORDS.pages(self.words.len()),
        };
        draw_footer(t, canvas, &footer, MARGIN);
    }

    pub(super) fn draw_word_button(&self, canvas: &mut Canvas, word: &str) {
        let label = if self.is_saved(word) {
            Msg::UnsaveWord
        } else {
            Msg::SaveWord
        };
        draw_button(&self.text, canvas, self.word_button(), &tr(&label), 34.0);
    }
}
