//! Geçici metin çizimi: harf birleştirme (shaping) ve kerning yok.
//! Yalnızca test ekranı içindir; asıl layout motoru M3'te seçilecek.

use crate::canvas::Canvas;
use crate::i18n::{Msg, tr};

pub struct Font {
    inner: fontdue::Font,
}

impl Font {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let inner = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()?;
        Some(Self { inner })
    }

    /// Arayüz metni çizmenin tek yolu: yalnızca katalogdaki mesajlar.
    pub fn draw_msg(&self, canvas: &mut Canvas, msg: &Msg, x: i32, baseline: i32, px: f32) {
        self.draw_str(canvas, &tr(msg), x, baseline, px);
    }

    fn draw_str(&self, canvas: &mut Canvas, text: &str, x: i32, baseline: i32, px: f32) {
        let mut pen = x as f32;
        for ch in text.chars() {
            let (m, coverage) = self.inner.rasterize(ch, px);
            let left = pen.round() as i32 + m.xmin;
            let top = baseline - m.height as i32 - m.ymin;
            for row in 0..m.height {
                for col in 0..m.width {
                    let a = coverage[row * m.width + col];
                    if a > 0 {
                        canvas.darken(left + col as i32, top + row as i32, 255 - a);
                    }
                }
            }
            pen += m.advance_width;
        }
    }
}

#[cfg(test)]
pub(crate) fn test_font() -> Font {
    let path = std::env::var("LEGENTIA_FONT")
        .expect("LEGENTIA_FONT ayarlı değil; testleri `nix develop` içinde çalıştırın");
    Font::from_bytes(&std::fs::read(path).expect("font okunamadı")).expect("geçersiz font")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turkish_capital_dotted_i_is_drawn_with_its_dot() {
        let font = test_font();
        let mut with_dot = Canvas::new(60, 60);
        let mut without = Canvas::new(60, 60);
        font.draw_str(&mut with_dot, "İ", 10, 50, 40.0);
        font.draw_str(&mut without, "I", 10, 50, 40.0);
        let ink = |c: &Canvas| c.pixels().iter().filter(|&&p| p < 128).count();
        assert!(ink(&without) > 0);
        assert!(ink(&with_dot) > ink(&without), "İ noktası çizilmemiş");
    }
}
