//! Okuma hızı ve kalan süre tahmini. Hız kullanıcının kendi sayfa
//! çevirmelerinden öğrenilir ve kitaplar arasında saklanır (`okuma-hizi.json`):
//! metinde saniyede karakter, PDF sayfa görüntüsünde ekran başına saniye.
//! Sayfada 3 sn ile 10 dk arası kalınıp ileri çevrilen sayfalar sayılır;
//! daha kısası atlama, daha uzunu ara verme sayılır.
//!
//! Kitabın kalanı EPUB'da bölüm dosyalarının boyutundan tahmin edilir: açık
//! bölümde ölçülen karakter/bayt oranı kalan bölümlerin boyutuna uygulanır.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::session::Doc;
use super::{App, View};
use crate::i18n::Msg;

pub(super) const PACE_FILE: &str = "okuma-hizi.json";
const MIN_DWELL: Duration = Duration::from_secs(3);
const MAX_DWELL: Duration = Duration::from_secs(600);
/// Tahmin bu kadar ölçümden sonra gösterilir.
const MIN_SAMPLES: u32 = 3;
/// Yeni ölçümün ortalamadaki ağırlığı.
const ALPHA: f32 = 0.2;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pace {
    pub chars_per_sec: f32,
    pub text_samples: u32,
    pub secs_per_screen: f32,
    pub screen_samples: u32,
}

fn mix(avg: &mut f32, n: &mut u32, value: f32) {
    *avg = if *n == 0 {
        value
    } else {
        *avg * (1.0 - ALPHA) + value * ALPHA
    };
    *n += 1;
}

impl Pace {
    pub fn record_text(&mut self, chars: usize, secs: f32) {
        if chars > 0 {
            mix(
                &mut self.chars_per_sec,
                &mut self.text_samples,
                chars as f32 / secs,
            );
        }
    }

    pub fn record_screen(&mut self, secs: f32) {
        mix(&mut self.secs_per_screen, &mut self.screen_samples, secs);
    }

    /// `chars` karakteri okuma süresi (sn); henüz yeterli ölçüm yoksa `None`.
    pub fn text_secs(&self, chars: f32) -> Option<f32> {
        (self.text_samples >= MIN_SAMPLES && self.chars_per_sec > 0.0)
            .then(|| chars / self.chars_per_sec)
    }

    pub fn screen_secs(&self, screens: f32) -> Option<f32> {
        (self.screen_samples >= MIN_SAMPLES).then_some(screens * self.secs_per_screen)
    }
}

fn minutes(secs: f32) -> u32 {
    (secs / 60.0).ceil().max(1.0) as u32
}

impl App {
    /// Sayfa değişmeden hemen önce: ileri çevriliyorsa sayfada geçen süre
    /// ölçüme katılır; sayaç yeni sayfa için baştan başlar.
    pub(super) fn note_dwell(&mut self, forward: bool) {
        let now = Instant::now();
        let shown = self.page_shown.replace(now);
        let (Some(shown), Some(s)) = (shown, &self.session) else {
            return;
        };
        let dwell = now.duration_since(shown);
        if !forward || !(MIN_DWELL..=MAX_DWELL).contains(&dwell) {
            return;
        }
        let secs = dwell.as_secs_f32();
        if s.showing_pdf_image() {
            self.pace.record_screen(secs);
        } else {
            // Yatayda iki sütun birlikte okunur.
            let chars = (s.page..s.page + s.columns).map(|p| s.page_chars(p)).sum();
            self.pace.record_text(chars, secs);
        }
        self.pace_dirty = true;
    }

    /// Okuma ekranından çıkınca (menü, sözlük…) geçen süre sayılmaz.
    pub(super) fn pause_dwell(&mut self) {
        if !matches!(self.view, View::Reading) {
            self.page_shown = None;
        }
    }

    /// Bölümün ve kitabın kalan okuma süresi (dakika, yukarı yuvarlanmış).
    pub(super) fn time_left(&self) -> Option<(u32, u32)> {
        let s = self.session.as_ref()?;
        let later = s.chapter_count().saturating_sub(s.spine + 1) as f32;
        let (chapter, rest) = if s.showing_pdf_image() {
            let screens = s.pages.len() as f32;
            let left = screens - s.page as f32;
            (
                self.pace.screen_secs(left)?,
                self.pace.screen_secs(later * screens)?,
            )
        } else {
            let left: usize = (s.page..s.pages.len()).map(|p| s.page_chars(p)).sum();
            let total: usize = (0..s.pages.len()).map(|p| s.page_chars(p)).sum();
            let rest_chars = match &s.doc {
                Doc::Epub(book) => {
                    let size = book.sizes.get(s.spine).copied().unwrap_or(0);
                    let rest: u64 = book.sizes.iter().skip(s.spine + 1).sum();
                    if size == 0 {
                        0.0
                    } else {
                        total as f32 / size as f32 * rest as f32
                    }
                }
                // Yeniden akıtılan PDF: her sayfa bu sayfa kadar.
                Doc::Pdf(_) => total as f32 * later,
            };
            (
                self.pace.text_secs(left as f32)?,
                self.pace.text_secs(rest_chars)?,
            )
        };
        Some((minutes(chapter), minutes(chapter + rest)))
    }

    /// Alt bilgi; okuma hızı biliniyorsa kalan sürelerle.
    pub(super) fn with_time_left(&self, footer: Msg) -> Msg {
        match self.time_left() {
            Some((chapter, book)) => Msg::TimeLeft {
                footer: Box::new(footer),
                chapter,
                book,
            },
            None => footer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pace_needs_a_few_samples_and_follows_recent_speed() {
        let mut p = Pace::default();
        p.record_text(600, 60.0);
        p.record_text(600, 60.0);
        assert_eq!(p.text_secs(600.0), None, "iki ölçüm yetmez");
        p.record_text(600, 60.0);
        assert_eq!(p.text_secs(600.0), Some(60.0));
        // Daha hızlı okuma ortalamayı yavaşça çeker.
        p.record_text(1200, 60.0);
        assert!((p.chars_per_sec - 12.0).abs() < 0.01, "{}", p.chars_per_sec);
        assert_eq!(minutes(1.0), 1);
        assert_eq!(minutes(61.0), 2);
    }
}
