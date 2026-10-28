//! Otomatik sayfa çevirme: alt çubuktaki "Otomatik çevir" ile başlar; ayarlı
//! aralıkla (ya da okuma hızına göre: sayfadaki karakter / öğrenilen hız)
//! sayfa ileri çevrilir. Dokunma ya da tuş durdurur (o dokunma başka bir şey
//! yapmaz); kitap bitince kendiliğinden durur. Çalışırken platform `Tick`
//! gönderir. Otomatik çevirmeler okuma hızı ölçümüne katılmaz.

use std::time::{Duration, Instant};

use super::{App, View};
use crate::event::{Event, PageKey, Refresh};

/// Okuma hızı henüz bilinmiyorsa aralık.
const FALLBACK: Duration = Duration::from_secs(30);

impl App {
    pub(super) fn start_auto_turn(&mut self) {
        self.auto_turn = Some(Instant::now());
    }

    pub(super) fn stop_auto_turn(&mut self) {
        self.auto_turn = None;
    }

    /// Okunan sayfanın otomatik çevrilme aralığı.
    fn auto_interval(&self) -> Duration {
        match self.settings.auto_turn {
            0 => self
                .session
                .as_ref()
                .and_then(|s| {
                    let chars: usize = (s.page..s.page + s.columns).map(|p| s.page_chars(p)).sum();
                    self.pace.text_secs(chars as f32)
                })
                .map_or(FALLBACK, |secs| Duration::from_secs_f32(secs.max(3.0))),
            secs => Duration::from_secs(secs as u64),
        }
    }

    /// `Event::Tick`: süre dolduysa sayfayı ileri çevirir.
    pub(super) fn poll_auto_turn(&mut self) -> Option<Refresh> {
        let since = self.auto_turn?;
        if !matches!(self.view, View::Reading) || since.elapsed() < self.auto_interval() {
            return None;
        }
        // Süre istatistiğe sayılır ama okuma hızı ölçümüne katılmaz: hız
        // ayarlanan aralığı öğrenmesin.
        self.flush_reading(true);
        self.page_shown = None;
        let refresh = self.handle_reading(Event::Key(PageKey::Forward));
        self.page_shown = Some(Instant::now());
        match refresh {
            Some(r) => {
                self.auto_turn = Some(Instant::now());
                Some(r)
            }
            None => {
                // Kitabın sonu.
                self.stop_auto_turn();
                None
            }
        }
    }
}
