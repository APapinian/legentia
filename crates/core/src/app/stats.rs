//! Okuma istatistikleri (`istatistik.json`): okuma ekranında geçen süre ve
//! okunan sayfalar, güne ve kitaba göre. Sayfada 10 dakikadan uzun kalmak
//! ara verme sayılır, süreye eklenmez. Günler UTC'ye göre ayrılır.
//! Kütüphane → Araçlar → Okuma istatistikleri ekranında gösterilir.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::pace::{MAX_DWELL, MIN_DWELL};
use super::ui::{
    BOLD, HEADER_BOTTOM, MARGIN, PLAIN, ROW_PX, back_button, draw_button, draw_header, one_line,
};
use super::{App, View};
use crate::canvas::Canvas;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};
use crate::text::Family;

pub(super) const STATS_FILE: &str = "istatistik.json";
/// Grafikte gösterilen gün sayısı.
const CHART_DAYS: u64 = 14;
const CHART_H: i32 = 260;
/// Listelenen en çok okunan kitap sayısı.
const TOP_BOOKS: usize = 5;
/// Kelime/dakika tahmininde bir kelimenin karakter sayısı (boşlukla).
const CHARS_PER_WORD: f32 = 7.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Tally {
    pub secs: u64,
    pub pages: u32,
}

impl Tally {
    fn add(&mut self, other: Tally) {
        self.secs += other.secs;
        self.pages += other.pages;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Stats {
    /// Gün sırası (1970-01-01'den beri, UTC) → o günün toplamı.
    pub days: BTreeMap<u64, Tally>,
    /// Kitap kimliği → toplam.
    pub books: HashMap<String, Tally>,
}

impl Stats {
    pub fn record(&mut self, day: u64, book: &str, t: Tally) {
        self.days.entry(day).or_default().add(t);
        self.books.entry(book.to_string()).or_default().add(t);
    }

    /// `today` dahil son `n` günün toplamı.
    fn last_days(&self, today: u64, n: u64) -> Tally {
        let mut sum = Tally::default();
        for (_, t) in self.days.range(today.saturating_sub(n - 1)..=today) {
            sum.add(*t);
        }
        sum
    }

    fn total(&self) -> Tally {
        let mut sum = Tally::default();
        for t in self.days.values() {
            sum.add(*t);
        }
        sum
    }

    /// Art arda okunan gün sayısı: bugün okunmadıysa dünden geriye.
    fn streak(&self, today: u64) -> u32 {
        let read = |d: u64| self.days.get(&d).is_some_and(|t| t.secs > 0);
        let mut day = if read(today) {
            today
        } else {
            today.saturating_sub(1)
        };
        let mut n = 0;
        while read(day) {
            n += 1;
            if day == 0 {
                break;
            }
            day -= 1;
        }
        n
    }
}

pub(super) fn today() -> u64 {
    super::annotations::now() / 86_400
}

/// Gün sırasının ayın kaçı olduğu (Hinnant'ın takvim algoritması).
fn day_of_month(day: u64) -> u32 {
    let z = day as i64 + 719_468;
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    (doy - (153 * mp + 2) / 5 + 1) as u32
}

impl App {
    /// Okunan sayfada geçen süreyi istatistiğe ekler ve sayacı yeniden
    /// başlatır; `page` sayfa bitti demek (ileri çevrildi). Geçen süre döner.
    pub(super) fn flush_reading(&mut self, page: bool) -> Option<Duration> {
        let now = Instant::now();
        let dwell = now.duration_since(self.page_shown.replace(now)?);
        if dwell <= MAX_DWELL
            && let Some(s) = &self.session
        {
            let t = Tally {
                secs: dwell.as_secs(),
                pages: u32::from(page && dwell >= MIN_DWELL),
            };
            self.stats.record(today(), &s.id, t);
            self.stats_dirty = true;
        }
        Some(dwell)
    }

    pub(super) fn save_stats(&mut self) {
        if self.stats_dirty {
            self.stats_dirty = false;
            self.store.save(STATS_FILE, &self.stats);
        }
    }

    pub(super) fn handle_stats(&mut self, event: Event) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        if !back_button(self.width).contains(x, y) {
            return None;
        }
        self.view = View::Backup;
        Some(self.refresh(true))
    }

    pub(super) fn draw_stats(&self, canvas: &mut Canvas) {
        let t = &self.text;
        draw_header(t, canvas, &tr(&Msg::Stats), "", true);
        let today = today();
        let mut y = HEADER_BOTTOM + 70;
        let mut line = |canvas: &mut Canvas, msg: &Msg| {
            t.draw_msg(canvas, msg, MARGIN, y, ROW_PX);
            y += 58;
        };
        let tally = |t: Tally| (t.secs, t.pages);
        let (secs, pages) = tally(self.stats.last_days(today, 1));
        line(canvas, &Msg::StatToday { secs, pages });
        let (secs, pages) = tally(self.stats.last_days(today, 7));
        line(canvas, &Msg::StatWeek { secs, pages });
        let (secs, pages) = tally(self.stats.total());
        line(canvas, &Msg::StatTotal { secs, pages });
        line(canvas, &Msg::StatStreak(self.stats.streak(today)));
        // Bir kelimenin okunma süresinden dakikadaki kelime.
        if let Some(secs) = self.pace.text_secs(CHARS_PER_WORD) {
            line(canvas, &Msg::StatSpeed((60.0 / secs).round() as u32));
        }

        // Son 14 günün çubuk grafiği (dakika).
        let top = y + 20;
        t.draw_msg(canvas, &Msg::StatChart, MARGIN, top, 30.0);
        let base = top + 40 + CHART_H;
        let first = today.saturating_sub(CHART_DAYS - 1);
        let minutes: Vec<u64> = (first..=today)
            .map(|d| self.stats.days.get(&d).map_or(0, |t| t.secs / 60))
            .collect();
        let most = minutes.iter().copied().max().unwrap_or(0).max(1);
        let slot = (canvas.width() as i32 - 2 * MARGIN) / CHART_DAYS as i32;
        for (i, &m) in minutes.iter().enumerate() {
            let x = MARGIN + i as i32 * slot;
            let h = (m as i32 * CHART_H / most as i32).max(i32::from(m > 0) * 3);
            canvas.fill_rect(x + 8, base - h, (slot - 16) as u32, h as u32, 60);
            let label = day_of_month(first + i as u64).to_string();
            let w = t.width_plain(&label, Family::Ui, PLAIN, 24.0) as i32;
            t.draw_plain(
                canvas,
                &label,
                Family::Ui,
                PLAIN,
                x + (slot - w) / 2,
                base + 34,
                24.0,
            );
        }
        canvas.fill_rect(
            MARGIN,
            base,
            (canvas.width() as i32 - 2 * MARGIN) as u32,
            2,
            0,
        );
        let most_label = tr(&Msg::Minutes(most as u32));
        t.draw_plain(
            canvas,
            &most_label,
            Family::Ui,
            PLAIN,
            MARGIN,
            top + 40,
            24.0,
        );

        // En çok okunan kitaplar.
        let mut books: Vec<(&String, &Tally)> = self
            .stats
            .books
            .iter()
            .filter(|(_, t)| t.secs >= 60)
            .collect();
        books.sort_by_key(|(_, t)| std::cmp::Reverse(t.secs));
        let mut y = base + 110;
        if !books.is_empty() {
            t.draw_msg(canvas, &Msg::StatBooks, MARGIN, y, 30.0);
            y += 56;
        }
        let right = canvas.width() as i32 - MARGIN;
        for (id, tally) in books.into_iter().take(TOP_BOOKS) {
            let title = self
                .library
                .iter()
                .find(|e| &e.id == id)
                .map_or_else(|| tr(&Msg::RemovedBook), |e| e.display_title());
            let time = tr(&Msg::Duration(tally.secs));
            let tw = t.width_plain(&time, Family::Ui, BOLD, 30.0) as i32;
            let title = one_line(t, &title, PLAIN, 30.0, (right - MARGIN - tw - 30) as f32);
            t.draw_plain(canvas, &title, Family::Ui, PLAIN, MARGIN, y, 30.0);
            t.draw_plain(canvas, &time, Family::Ui, BOLD, right - tw, y, 30.0);
            y += 50;
        }
        draw_button(
            t,
            canvas,
            back_button(canvas.width()),
            &tr(&Msg::Back),
            ROW_PX,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_streaks_and_calendar_days() {
        let mut s = Stats::default();
        let t = |secs, pages| Tally { secs, pages };
        s.record(100, "a", t(600, 5));
        s.record(101, "a", t(60, 1));
        s.record(102, "b", t(120, 2));
        s.record(104, "b", t(30, 0));
        assert_eq!(s.last_days(104, 1), t(30, 0));
        assert_eq!(s.last_days(104, 3), t(150, 2));
        assert_eq!(s.total(), t(810, 8));
        assert_eq!(s.books["a"], t(660, 6));
        assert_eq!(s.streak(104), 1);
        assert_eq!(s.streak(103), 3, "bugün okunmadıysa dünden sayılır");
        assert_eq!(s.streak(106), 0);
        // 1970-01-01 = 0; 2026-10-28 = 20754.
        assert_eq!(day_of_month(0), 1);
        assert_eq!(day_of_month(20_754), 28);
        assert_eq!(day_of_month(20_758), 1, "2026-11-01");
    }
}
