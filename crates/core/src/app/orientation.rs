//! Okurken ekran yönü. Ayara göre (otomatikte platformun ivmeölçerinden
//! gelen `Tilt` olayına göre) okuma ekranı ve üstüne açılanlar (menü
//! çubukları, dipnot, seçim, tam ekran görsel) yataya döner; ayarlar,
//! listeler, formlar ve sözlük dikeyde kalır. Platform her olaydan sonra
//! `screen_size`'a bakıp ekranı döndürür.

use super::settings::Orientation;
use super::{App, View};
use crate::LIBRA2_HEIGHT;

impl App {
    /// Geçerli ekran boyutu (genişlik, yükseklik); platform tuvali bu boyutta açar.
    pub fn screen_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Okuma ekranı ya da üstüne açılan bir katman mı (yalnızca bunlar döner)?
    fn reading_view(&self) -> bool {
        self.session.is_some()
            && matches!(
                self.view,
                View::Reading
                    | View::Menu
                    | View::Footnote
                    | View::Selected
                    | View::Annotation { .. }
                    | View::Image { .. }
            )
    }

    fn wants_landscape(&self) -> bool {
        self.reading_view()
            && match self.settings.orientation {
                Orientation::Auto => self.tilted,
                Orientation::Portrait => false,
                Orientation::Landscape => true,
            }
    }

    /// Ekran boyutunu istenen yöne getirir; okuma ekranındaysa bölüm o
    /// boyuta göre (gerekirse) yeniden dizilir. Boyut değiştiyse `true`.
    pub(super) fn fit_orientation(&mut self) -> bool {
        let size = if self.wants_landscape() {
            (LIBRA2_HEIGHT, self.portrait_width)
        } else {
            (self.portrait_width, LIBRA2_HEIGHT)
        };
        let resized = size != (self.width, self.height);
        if resized {
            (self.width, self.height) = size;
            self.pdf_frame.replace(None);
            self.pdf_right.replace(None);
        }
        if self.reading_view()
            && let Some(s) = self.session.as_mut()
            && !s.pages.is_empty()
            && s.laid_out != size
        {
            s.relayout(&self.text, &self.settings, size);
        }
        resized
    }
}
