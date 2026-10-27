//! Geçici metin çizimi: harf birleştirme (shaping), kerning ve heceleme yok;
//! satır sarma kelime sınırında açgözlü yapılır. Asıl layout motoru M3'te seçilecek.

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
        self.draw_content(canvas, &tr(msg), x, baseline, px);
    }

    pub(crate) fn width(&self, text: &str, px: f32) -> f32 {
        text.chars()
            .map(|c| self.inner.metrics(c, px).advance_width)
            .sum()
    }

    /// Metni `max_width` genişliğinde satırlara böler. Satıra sığmayan tek
    /// kelime harf harf bölünür.
    pub(crate) fn wrap(&self, text: &str, px: f32, max_width: f32) -> Vec<String> {
        let space = self.width(" ", px);
        let mut lines = Vec::new();
        let mut line = String::new();
        let mut line_w = 0.0;
        for word in text.split_whitespace() {
            let w = self.width(word, px);
            if !line.is_empty() && line_w + space + w <= max_width {
                line.push(' ');
                line.push_str(word);
                line_w += space + w;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            if w <= max_width {
                line.push_str(word);
                line_w = w;
                continue;
            }
            line_w = 0.0;
            for c in word.chars() {
                let cw = self.inner.metrics(c, px).advance_width;
                if !line.is_empty() && line_w + cw > max_width {
                    lines.push(std::mem::take(&mut line));
                    line_w = 0.0;
                }
                line.push(c);
                line_w += cw;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }

    /// Kitap içeriğini (arayüz metni değil) çizer.
    pub(crate) fn draw_content(
        &self,
        canvas: &mut Canvas,
        text: &str,
        x: i32,
        baseline: i32,
        px: f32,
    ) {
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
        font.draw_content(&mut with_dot, "İ", 10, 50, 40.0);
        font.draw_content(&mut without, "I", 10, 50, 40.0);
        let ink = |c: &Canvas| c.pixels().iter().filter(|&&p| p < 128).count();
        assert!(ink(&without) > 0);
        assert!(ink(&with_dot) > ink(&without), "İ noktası çizilmemiş");
    }

    #[test]
    fn wrap_fits_every_line_and_keeps_all_words() {
        let font = test_font();
        let text = "Ağaçların gölgesinde oturup çayımızı içerken ışığın yaprakların arasından süzülüşünü izledik.";
        let lines = font.wrap(text, 34.0, 400.0);
        assert!(lines.len() > 1);
        for l in &lines {
            assert!(font.width(l, 34.0) <= 400.0, "taşan satır: {l}");
        }
        assert_eq!(lines.join(" "), text);
    }

    #[test]
    fn wrap_splits_a_word_longer_than_the_line() {
        let font = test_font();
        let lines = font.wrap("Çekoslovakyalılaştıramadıklarımızdanmışsınız", 34.0, 200.0);
        assert!(lines.len() > 1);
        assert_eq!(
            lines.concat(),
            "Çekoslovakyalılaştıramadıklarımızdanmışsınız"
        );
    }
}
