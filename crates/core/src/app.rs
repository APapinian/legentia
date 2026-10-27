use crate::canvas::Canvas;
use crate::event::{Event, PageKey, Refresh};
use crate::i18n::Msg;
use crate::text::Font;

/// Kaç kısmi yenilemeden sonra bir tam yenileme yapılacağı.
/// Geçici değer; gerçek cihazda gölgelenmeye bakılarak ayarlanacak.
const PARTIALS_PER_FULL: u32 = 6;

/// M1 test ekranı: dokunma ve tuş olaylarını gösterir.
pub struct App {
    font: Font,
    last_tap: Option<(u32, u32)>,
    back: u32,
    forward: u32,
    partial: u32,
    full: u32,
    since_full: u32,
}

impl App {
    pub fn new(font: Font) -> Self {
        Self {
            font,
            last_tap: None,
            back: 0,
            forward: 0,
            partial: 0,
            full: 0,
            since_full: 0,
        }
    }

    /// İlk açılışta ekran her zaman tam yenilenir.
    pub fn start(&mut self) -> Refresh {
        self.count(Refresh::Full)
    }

    pub fn handle(&mut self, event: Event) -> Option<Refresh> {
        match event {
            Event::Tap { x, y } => self.last_tap = Some((x, y)),
            Event::Key(PageKey::Back) => self.back += 1,
            Event::Key(PageKey::Forward) => self.forward += 1,
        }
        let refresh = if self.since_full + 1 >= PARTIALS_PER_FULL {
            Refresh::Full
        } else {
            Refresh::Partial
        };
        Some(self.count(refresh))
    }

    fn count(&mut self, refresh: Refresh) -> Refresh {
        match refresh {
            Refresh::Full => {
                self.full += 1;
                self.since_full = 0;
            }
            Refresh::Partial => {
                self.partial += 1;
                self.since_full += 1;
            }
        }
        refresh
    }

    pub fn draw(&self, canvas: &mut Canvas) {
        canvas.fill(255);
        let f = &self.font;
        let x = 80;
        f.draw_msg(canvas, &Msg::TestTitle, x, 160, 64.0);
        canvas.fill_rect(x, 190, canvas.width() - 2 * x as u32, 3, 0);
        f.draw_msg(canvas, &Msg::TurkishSample, x, 290, 40.0);
        f.draw_msg(canvas, &Msg::TestHint, x, 380, 36.0);

        let tap = match self.last_tap {
            Some((x, y)) => Msg::LastTap { x, y },
            None => Msg::NoTapYet,
        };
        f.draw_msg(canvas, &tap, x, 500, 36.0);
        let keys = Msg::PageKeys {
            back: self.back,
            forward: self.forward,
        };
        f.draw_msg(canvas, &keys, x, 570, 36.0);
        let refreshes = Msg::Refreshes {
            partial: self.partial,
            full: self.full,
        };
        f.draw_msg(canvas, &refreshes, x, 640, 36.0);

        // Dokunulan noktada artı işareti: koordinat dönüşümünü gözle doğrulamak için.
        if let Some((tx, ty)) = self.last_tap {
            let (tx, ty) = (tx as i32, ty as i32);
            canvas.fill_rect(tx - 30, ty - 2, 60, 5, 0);
            canvas.fill_rect(tx - 2, ty - 30, 5, 60, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::test_font;

    #[test]
    fn start_is_full_refresh() {
        let mut app = App::new(test_font());
        assert_eq!(app.start(), Refresh::Full);
    }

    #[test]
    fn every_sixth_refresh_is_full() {
        let mut app = App::new(test_font());
        app.start();
        let kinds: Vec<_> = (0..12)
            .map(|_| app.handle(Event::Key(PageKey::Forward)).unwrap())
            .collect();
        let fulls: Vec<_> = kinds
            .iter()
            .enumerate()
            .filter(|(_, r)| **r == Refresh::Full)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(fulls, vec![5, 11]);
        assert_eq!(app.forward, 12);
    }

    #[test]
    fn tap_is_remembered_and_marked() {
        let mut app = App::new(test_font());
        app.handle(Event::Tap { x: 600, y: 1200 });
        assert_eq!(app.last_tap, Some((600, 1200)));
        let mut c = Canvas::new(crate::LIBRA2_WIDTH, crate::LIBRA2_HEIGHT);
        app.draw(&mut c);
        let at = |x: u32, y: u32| c.pixels()[(y * c.width() + x) as usize];
        assert_eq!(at(600, 1200), 0);
    }
}
