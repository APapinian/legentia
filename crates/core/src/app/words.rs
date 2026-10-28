//! Kişisel kelime listesi: sözlükte bakılan kelimeler, sözlükteki biçimleri,
//! bulundukları kitap ve eklenme zamanıyla (`words.json`).

use serde::{Deserialize, Serialize};

use super::screens::WORDS;
use super::ui::{
    MARGIN, PLAIN, ROW_PX, Rect, back_button, draw_button, draw_footer, draw_header, header_button,
    one_line,
};
use super::{App, View};
use crate::LIBRA2_HEIGHT;
use crate::canvas::Canvas;
use crate::collate;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};
use crate::text::Family;

pub(super) const WORDS_FILE: &str = "words.json";
/// Anki'nin içe aktardığı kart dosyası (kütüphane klasöründe).
pub(super) const ANKI_FILE: &str = "Legentia kelimeler.csv";
/// Kartın arka yüzündeki tanımın en fazla karakter sayısı.
const ANKI_DEFINITION_CHARS: usize = 600;

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

    /// Başlıktaki "Anki'ye aktar" düğmesi.
    fn anki_button(&self) -> Rect {
        header_button(self.width, 280)
    }

    /// Kelime listesini Anki kartları olarak (CSV: kelime; tanım ve kaynak)
    /// kütüphane klasörüne yazar. Tanım cihazdaki sözlüklerden gelir.
    pub(super) fn export_anki(&mut self) -> Msg {
        let words = self.words.clone();
        let mut csv = String::from("#separator:Semicolon\n#html:false\n#columns:Ön;Arka\n");
        for w in &words {
            let definition = self.plain_definition(&w.word);
            let mut back = definition;
            let source =
                if w.seen.is_empty() || crate::dict::key(&w.seen) == crate::dict::key(&w.word) {
                    w.book.clone()
                } else {
                    format!("“{}” · {}", w.seen, w.book)
                };
            if !source.is_empty() {
                if !back.is_empty() {
                    back.push_str(" — ");
                }
                back.push_str(&source);
            }
            csv.push_str(&format!("{};{}\n", csv_field(&w.word), csv_field(&back)));
        }
        match std::fs::write(self.root.join(ANKI_FILE), csv) {
            Ok(()) => Msg::AnkiExported(words.len()),
            Err(e) => {
                log::error!("cannot write {ANKI_FILE}: {e}");
                Msg::ExportFailed
            }
        }
    }

    /// Kelimenin cihazdaki sözlüklerdeki ilk tanımı, tek satır düz metin.
    fn plain_definition(&mut self, word: &str) -> String {
        let dicts = self.dictionaries();
        let Some(def) = dicts.define(word) else {
            return String::new();
        };
        let Some((name, article)) = def.articles.first() else {
            return String::new();
        };
        let text: Vec<String> = crate::dict::render::article_blocks(name, article)
            .into_iter()
            .skip(2) // madde başı ve sözlük adı
            .map(|b| b.text)
            .collect();
        let text = text
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if text.chars().count() > ANKI_DEFINITION_CHARS {
            text.chars().take(ANKI_DEFINITION_CHARS).collect::<String>() + "…"
        } else {
            text
        }
    }

    pub(super) fn handle_words(&mut self, page: usize, event: Event) -> Option<Refresh> {
        if let Event::Tap { x, y } = event {
            if back_button(self.width).contains(x, y) {
                self.view = View::Reading;
                return Some(self.refresh(true));
            }
            if !self.words.is_empty() && self.anki_button().contains(x, y) {
                let msg = self.export_anki();
                self.view = View::Message {
                    msg,
                    back: super::Back::Reading,
                };
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
        if !self.words.is_empty() {
            draw_button(t, canvas, self.anki_button(), &tr(&Msg::ExportAnki), ROW_PX);
        }
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
/// CSV alanı: noktalı virgül, tırnak ya da satır sonu içeriyorsa tırnaklanır.
fn csv_field(s: &str) -> String {
    let s = s.replace(['\n', '\r'], " ");
    if s.contains([';', '"']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}
