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
    /// Aralıklı tekrar durumu; yeni kelime hemen tekrara girer.
    #[serde(default)]
    pub review: Review,
}

/// Kartın aralıklı tekrar durumu (SM-2'ye benzer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Review {
    /// Kartın yeniden sorulacağı gün (1970'ten beri, UTC; bkz. `stats::today`).
    pub due: u64,
    /// Son aralık (gün).
    pub interval: u32,
    /// Kolaylık çarpanı (binde; 2500 = 2,5 kat).
    pub ease: u32,
    /// Art arda bilinme sayısı.
    pub streak: u32,
}

impl Default for Review {
    fn default() -> Self {
        Review {
            due: 0,
            interval: 0,
            ease: 2500,
            streak: 0,
        }
    }
}

/// Tekrardaki yanıt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Grade {
    Again,
    Hard,
    Good,
}

const GRADES: [Grade; 3] = [Grade::Again, Grade::Hard, Grade::Good];

impl Review {
    /// Yanıta göre sonraki gün: bilinmeyen bugün yeniden sorulur, zor kartın
    /// aralığı az uzar ve kolaylığı düşer, bilinen 1, 3 gün sonra, sonra
    /// aralık × kolaylık.
    pub(super) fn answer(self, grade: Grade, today: u64) -> Review {
        let mut r = self;
        match grade {
            Grade::Again => {
                r.streak = 0;
                r.interval = 0;
                r.ease = r.ease.saturating_sub(200).max(1300);
            }
            Grade::Hard => {
                r.interval = (r.interval * 12 / 10).max(1);
                r.ease = r.ease.saturating_sub(150).max(1300);
            }
            Grade::Good => {
                r.streak += 1;
                r.interval = match r.streak {
                    1 => 1,
                    2 => 3,
                    _ => {
                        (r.interval as u64 * r.ease as u64 / 1000).max(r.interval as u64 + 1) as u32
                    }
                };
            }
        }
        r.due = today + r.interval as u64;
        r
    }
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
                review: Review::default(),
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
        header_button(self.width, 260)
    }

    /// Başlıktaki "Tekrar et (N)" düğmesi, Anki düğmesinin solunda.
    pub(super) fn review_button(&self) -> Rect {
        super::ui::left_of(self.anki_button(), 230)
    }

    /// Bugün tekrar sırası gelmiş kelimeler (`words` sırası), en eskiden.
    pub(super) fn due_words(&self) -> Vec<usize> {
        let today = super::stats::today();
        let mut due: Vec<usize> = (0..self.words.len())
            .filter(|&i| self.words[i].review.due <= today)
            .collect();
        due.sort_by_key(|&i| (self.words[i].review.due, self.words[i].created));
        due
    }

    /// Sıradaki kart (az önce sorulandan başkası, yoksa o); kalmadıysa bitti ekranı.
    fn next_card(&mut self, last: Option<usize>, done: u32) -> Option<Refresh> {
        let due = self.due_words();
        let card = due
            .iter()
            .copied()
            .find(|&i| Some(i) != last)
            .or(due.first().copied());
        self.view = View::Review {
            card,
            answer: None,
            done,
        };
        Some(self.refresh(true))
    }

    /// Yanıt düğmeleri (Bilmedim, Zor, Bildim), ekranın altında.
    pub(super) fn grade_rect(&self, i: usize) -> Rect {
        let n = GRADES.len() as i32;
        let gap = 20;
        let w = (self.width as i32 - 2 * MARGIN - (n - 1) * gap) / n;
        Rect {
            x: MARGIN + i as i32 * (w + gap),
            y: LIBRA2_HEIGHT as i32 - 260,
            w,
            h: 120,
        }
    }

    pub(super) fn handle_review(
        &mut self,
        card: Option<usize>,
        answer: Option<String>,
        done: u32,
        event: Event,
    ) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        let back = back_button(self.width).contains(x, y);
        let Some(index) = card.filter(|_| !back) else {
            // Geri ya da bitti ekranına dokunma: kelime listesi.
            self.view = View::Words { page: 0 };
            return Some(self.refresh(true));
        };
        if answer.is_none() {
            // Kartı çevir: tanım cihazdaki sözlüklerden.
            let word = self.words.get(index)?.word.clone();
            let definition = self.plain_definition(&word);
            self.view = View::Review {
                card,
                answer: Some(definition),
                done,
            };
            return Some(self.refresh(false));
        }
        let grade = GRADES
            .iter()
            .enumerate()
            .find(|(i, _)| self.grade_rect(*i).contains(x, y))
            .map(|(_, g)| *g)?;
        let w = self.words.get_mut(index)?;
        w.review = w.review.answer(grade, super::stats::today());
        self.store.save(WORDS_FILE, &self.words);
        self.next_card(Some(index), done + u32::from(grade != Grade::Again))
    }

    pub(super) fn draw_review(
        &self,
        canvas: &mut Canvas,
        card: Option<usize>,
        answer: Option<&str>,
        done: u32,
    ) {
        let t = &self.text;
        let left = self.due_words().len();
        draw_header(
            t,
            canvas,
            &tr(&Msg::ReviewTitle),
            &tr(&Msg::ReviewLeft(left)),
            true,
        );
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
        let Some(w) = card.and_then(|i| self.words.get(i)) else {
            self.draw_message_lines(
                canvas,
                &Msg::ReviewDone(done),
                super::ui::HEADER_BOTTOM + 120,
            );
            return;
        };
        let mut y = super::ui::HEADER_BOTTOM + 200;
        for line in t.wrap_plain(&w.word, Family::Ui, super::ui::BOLD, 64.0, max_w) {
            t.draw_plain(canvas, &line, Family::Ui, super::ui::BOLD, MARGIN, y, 64.0);
            y += 80;
        }
        let Some(answer) = answer else {
            t.draw_msg(canvas, &Msg::ReviewTapToShow, MARGIN, y + 80, 32.0);
            return;
        };
        let source = if w.seen.is_empty() || crate::dict::key(&w.seen) == crate::dict::key(&w.word)
        {
            w.book.clone()
        } else {
            format!("“{}” · {}", w.seen, w.book)
        };
        y += 20;
        for line in t
            .wrap_plain(&source, Family::Ui, PLAIN, 30.0, max_w)
            .iter()
            .take(2)
        {
            t.draw_plain(canvas, line, Family::Ui, PLAIN, MARGIN, y, 30.0);
            y += 44;
        }
        y += 40;
        let text = if answer.is_empty() {
            tr(&Msg::ReviewNoDefinition)
        } else {
            answer.to_string()
        };
        let bottom = self.grade_rect(0).y - 40;
        for line in t.wrap_plain(&text, Family::Ui, PLAIN, ROW_PX, max_w) {
            if y > bottom {
                break;
            }
            t.draw_plain(canvas, &line, Family::Ui, PLAIN, MARGIN, y, ROW_PX);
            y += 52;
        }
        for (i, g) in GRADES.iter().enumerate() {
            let label = match g {
                Grade::Again => Msg::GradeAgain,
                Grade::Hard => Msg::GradeHard,
                Grade::Good => Msg::GradeGood,
            };
            draw_button(t, canvas, self.grade_rect(i), &tr(&label), ROW_PX);
        }
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
            if !self.words.is_empty() && self.review_button().contains(x, y) {
                return self.next_card(None, 0);
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
            let review = Msg::ReviewButton(self.due_words().len());
            draw_button(t, canvas, self.review_button(), &tr(&review), ROW_PX);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervals_grow_when_known_and_reset_when_forgotten() {
        let mut r = Review::default();
        let mut days = Vec::new();
        for _ in 0..4 {
            r = r.answer(Grade::Good, 100);
            days.push(r.interval);
        }
        assert_eq!(days, [1, 3, 7, 17]);
        assert_eq!(r.due, 117);
        let hard = r.answer(Grade::Hard, 200);
        assert_eq!((hard.interval, hard.ease), (20, 2350));
        let again = r.answer(Grade::Again, 200);
        assert_eq!((again.interval, again.due, again.streak), (0, 200, 0));
        assert_eq!(again.answer(Grade::Good, 200).interval, 1, "baştan");
    }
}
