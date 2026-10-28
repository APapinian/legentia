//! Metin motoru: rustybuzz ile harf birleştirme (shaping), swash ile glif
//! çizimi, unicode-linebreak ile satır kırma ve Türkçe heceleme.
//!
//! Şimdilik yalnızca soldan sağa yazı dizilir. Sağdan sola (Osmanlıca) ve
//! dikey/CJK için yer ayrıldı: glifler kaynak metindeki bayt konumunu
//! (`cluster`) taşır, satırlar bayt aralığıyla tanımlanır.

mod fonts;
mod hyphen;
mod layout;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::Format;

use crate::canvas::Canvas;
use crate::i18n::{Msg, tr};
pub(crate) use fonts::FaceId;
pub use fonts::{Family, FontError, Fonts, Style, book_family_count, book_family_name};
pub use layout::Span;
pub(crate) use layout::{Align, Line, ParaStyle, layout};

/// Dizilmiş tek glif. Konumlar piksel; `cluster` kaynak metindeki bayt konumu.
#[derive(Debug, Clone)]
pub(crate) struct Glyph {
    pub id: u16,
    pub face: FaceId,
    pub px: f32,
    pub cluster: usize,
    pub advance: f32,
    pub x_off: f32,
    pub y_off: f32,
    pub underline: bool,
}

struct Bitmap {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    data: Vec<u8>,
}

/// Bu sayıyı aşınca önbellek boşaltılır; cihaz belleği sınırlı.
const MAX_CACHED_GLYPHS: usize = 4096;

struct Cache {
    ctx: ScaleContext,
    glyphs: HashMap<(FaceId, u16, u32), Option<Bitmap>>,
}

pub struct Text {
    fonts: Fonts,
    cache: RefCell<Cache>,
    /// Kitap metninin kalınlaştırma düzeyi (0 = yok); arayüz yazısına uygulanmaz.
    weight: Cell<u8>,
}

/// Kalınlaştırma düzeylerinin gamma değerleri: yarı saydam kenarlar koyulaşır,
/// ince çizgiler E Ink'te daha belirgin görünür. 3. düzey ayrıca bir piksel
/// yatay genişletir. Çizilmiş glife uygulanır; bileşik gliflerde (Literata'nın
/// "i"si gibi) kontur kalınlaştırması bileşenleri atladığı için o kullanılmaz.
const WEIGHT_GAMMA: [f32; 4] = [1.0, 0.75, 0.55, 0.55];

/// Glif bitmap'ini `level` düzeyinde koyulaştırır; yeni genişliği döner.
fn thicken(data: &mut Vec<u8>, width: u32, height: u32, level: u8) -> u32 {
    let level = (level as usize).min(3);
    if level == 0 || width == 0 {
        return width;
    }
    let g = WEIGHT_GAMMA[level];
    for a in data.iter_mut() {
        *a = (255.0 * (*a as f32 / 255.0).powf(g)).round() as u8;
    }
    if level < 3 {
        return width;
    }
    let w = width as usize;
    let mut wide = vec![0u8; (w + 1) * height as usize];
    for row in 0..height as usize {
        let src = &data[row * w..][..w];
        let dst = &mut wide[row * (w + 1)..][..w + 1];
        for x in 0..=w {
            let here = src.get(x).copied().unwrap_or(0);
            let left = if x > 0 { src[x - 1] } else { 0 };
            dst[x] = here.max(left);
        }
    }
    *data = wide;
    width + 1
}

impl Text {
    pub fn new(fonts: Fonts) -> Self {
        Self {
            fonts,
            cache: RefCell::new(Cache {
                ctx: ScaleContext::new(),
                glyphs: HashMap::new(),
            }),
            weight: Cell::new(0),
        }
    }

    /// Kitap metninin kalınlığı (0–3). Değişince glif önbelleği boşaltılır.
    pub(crate) fn set_weight(&self, level: u8) {
        if self.weight.replace(level) != level {
            self.cache.borrow_mut().glyphs.clear();
        }
    }

    pub(crate) fn fonts(&self) -> &Fonts {
        &self.fonts
    }

    /// `text`'i dizip `out`'a ekler; `base`, `text`'in paragraf içindeki bayt konumu.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn shape(
        &self,
        text: &str,
        face: FaceId,
        px: f32,
        lang: Option<&str>,
        underline: bool,
        base: usize,
        out: &mut Vec<Glyph>,
    ) {
        let hb = self.fonts.shaper(face);
        let scale = px / hb.units_per_em() as f32;
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        if let Some(lang) = lang.and_then(|l| l.parse().ok()) {
            buffer.set_language(lang);
        }
        buffer.guess_segment_properties();
        let shaped = rustybuzz::shape(hb, &[], buffer);
        for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
            out.push(Glyph {
                id: info.glyph_id as u16,
                face,
                px,
                cluster: base + info.cluster as usize,
                advance: pos.x_advance as f32 * scale,
                x_off: pos.x_offset as f32 * scale,
                y_off: pos.y_offset as f32 * scale,
                underline,
            });
        }
    }

    /// Yazı tipinin bu boyuttaki yükselme ve alçalma değerleri (pozitif, piksel).
    pub(crate) fn ascent_descent(&self, face: FaceId, px: f32) -> (f32, f32) {
        match swash::FontRef::from_index(self.fonts.data(face), 0) {
            Some(font) => {
                let m = font.metrics(&[]).scale(px);
                (m.ascent, m.descent)
            }
            None => (px * 0.8, px * 0.2),
        }
    }

    fn draw_glyph(&self, canvas: &mut Canvas, g: &Glyph, x: f32, baseline: i32) {
        let mut cache = self.cache.borrow_mut();
        if cache.glyphs.len() > MAX_CACHED_GLYPHS {
            cache.glyphs.clear();
        }
        let Cache { ctx, glyphs } = &mut *cache;
        let key = (g.face, g.id, (g.px * 64.0) as u32);
        let level = if g.face.is_ui() { 0 } else { self.weight.get() };
        let bitmap = glyphs.entry(key).or_insert_with(|| {
            let font = swash::FontRef::from_index(self.fonts.data(g.face), 0)?;
            let mut scaler = ctx.builder(font).size(g.px).hint(true).build();
            let mut image = Render::new(&[Source::Outline])
                .format(Format::Alpha)
                .render(&mut scaler, g.id)?;
            let (w, h) = (image.placement.width, image.placement.height);
            let width = thicken(&mut image.data, w, h, level);
            Some(Bitmap {
                left: image.placement.left,
                top: image.placement.top,
                width,
                height: h,
                data: image.data,
            })
        });
        let Some(b) = bitmap else {
            return;
        };
        let x0 = (x + g.x_off).round() as i32 + b.left;
        let y0 = baseline - (g.y_off.round() as i32) - b.top;
        for row in 0..b.height {
            for col in 0..b.width {
                let a = b.data[(row * b.width + col) as usize];
                if a > 0 {
                    canvas.darken(x0 + col as i32, y0 + row as i32, 255 - a);
                }
            }
        }
    }

    /// Düzenlenmiş bir satırı `x` sol kenarından çizer.
    pub(crate) fn draw_line(&self, canvas: &mut Canvas, line: &Line, x: i32, baseline: i32) {
        for (g, gx) in &line.glyphs {
            self.draw_glyph(canvas, g, x as f32 + gx, baseline);
        }
        // Bağlantı altı çizgisi: ardışık altı çizili gliflerin kapladığı aralık.
        let mut run: Option<(f32, f32, f32)> = None;
        for (g, gx) in &line.glyphs {
            if g.underline {
                run = Some(match run {
                    Some((start, _, px)) => (start, gx + g.advance, px),
                    None => (*gx, gx + g.advance, g.px),
                });
            } else if let Some(r) = run.take() {
                underline(canvas, r, x, baseline);
            }
        }
        if let Some(r) = run {
            underline(canvas, r, x, baseline);
        }
    }

    /// Tek satırlık düz metin (sarılmaz).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_plain(
        &self,
        canvas: &mut Canvas,
        text: &str,
        family: Family,
        style: Style,
        x: i32,
        baseline: i32,
        px: f32,
    ) {
        let face = self.fonts.face(family, style);
        let mut glyphs = Vec::new();
        self.shape(text, face, px, Some("tr"), style.link, 0, &mut glyphs);
        let mut pen = 0.0;
        let glyphs = glyphs
            .into_iter()
            .map(|g| {
                let at = pen;
                pen += g.advance;
                (g, at)
            })
            .collect();
        let line = Line {
            glyphs,
            start: 0,
            end: text.len(),
        };
        self.draw_line(canvas, &line, x, baseline);
    }

    /// Arayüz metni çizmenin tek yolu: yalnızca katalogdaki mesajlar.
    pub fn draw_msg(&self, canvas: &mut Canvas, msg: &Msg, x: i32, baseline: i32, px: f32) {
        self.draw_plain(
            canvas,
            &tr(msg),
            Family::Ui,
            Style::default(),
            x,
            baseline,
            px,
        );
    }

    /// Tek satırlık düz metnin genişliği (piksel).
    pub(crate) fn width_plain(&self, text: &str, family: Family, style: Style, px: f32) -> f32 {
        let mut glyphs = Vec::new();
        self.shape(
            text,
            self.fonts.face(family, style),
            px,
            Some("tr"),
            false,
            0,
            &mut glyphs,
        );
        glyphs.iter().map(|g| g.advance).sum()
    }

    /// Düz metni `max_width` genişliğinde satırlara böler (heceleme yok).
    pub(crate) fn wrap_plain(
        &self,
        text: &str,
        family: Family,
        style: Style,
        px: f32,
        max_width: f32,
    ) -> Vec<String> {
        let para = ParaStyle {
            family,
            px,
            bold: style.bold,
            align: Align::Left,
            hyphenate: false,
            lang: Some("tr"),
        };
        let spans = [Span {
            start: 0,
            end: text.len(),
            style,
        }];
        layout(self, text, &spans, &para, max_width)
            .iter()
            .map(|l| text[l.start..l.end].trim_end().to_string())
            .collect()
    }
}

fn underline(canvas: &mut Canvas, (start, end, px): (f32, f32, f32), x: i32, baseline: i32) {
    let thickness = (px / 18.0).max(1.0) as u32;
    let y = baseline + (px * 0.12) as i32;
    canvas.fill_rect(
        x + start as i32,
        y,
        (end - start).max(0.0) as u32,
        thickness,
        0,
    );
}

#[cfg(test)]
pub(crate) fn test_text() -> Text {
    let dir = std::env::var("LEGENTIA_FONTS")
        .expect("LEGENTIA_FONTS ayarlı değil; testleri `nix develop` içinde çalıştırın");
    Text::new(Fonts::open(std::path::Path::new(&dir)).expect("fontlar yüklenemedi"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thickening_darkens_edges_and_widens_at_the_top_level() {
        let mut d = vec![0, 64, 255];
        assert_eq!(thicken(&mut d, 3, 1, 0), 3);
        assert_eq!(d, [0, 64, 255], "0: değişmez");
        assert_eq!(thicken(&mut d, 3, 1, 2), 3);
        assert!(d[1] > 64 && d[2] == 255 && d[0] == 0, "{d:?}");
        let mut d = vec![0, 255, 0, 0, 0, 255];
        assert_eq!(thicken(&mut d, 3, 2, 3), 4);
        assert_eq!(d, [0, 255, 255, 0, 0, 0, 255, 255]);
    }

    fn ink(c: &Canvas) -> usize {
        c.pixels().iter().filter(|&&p| p < 128).count()
    }

    #[test]
    fn turkish_capital_dotted_i_is_drawn_with_its_dot() {
        let t = test_text();
        let mut with_dot = Canvas::new(80, 80);
        let mut without = Canvas::new(80, 80);
        t.draw_plain(
            &mut with_dot,
            "İ",
            Family::Book(0),
            Style::default(),
            10,
            60,
            48.0,
        );
        t.draw_plain(
            &mut without,
            "I",
            Family::Book(0),
            Style::default(),
            10,
            60,
            48.0,
        );
        assert!(ink(&without) > 0);
        assert!(ink(&with_dot) > ink(&without), "İ noktası çizilmemiş");
    }

    #[test]
    fn every_family_and_style_covers_turkish_letters() {
        let t = test_text();
        let letters = "İıĞğÜüŞşÖöÇç";
        for fam in 0..book_family_count() {
            for (bold, italic) in [(false, false), (false, true), (true, false), (true, true)] {
                let style = Style {
                    bold,
                    italic,
                    link: false,
                };
                let face = t.fonts().face(Family::Book(fam), style);
                let mut glyphs = Vec::new();
                t.shape(letters, face, 40.0, Some("tr"), false, 0, &mut glyphs);
                assert!(
                    glyphs.iter().all(|g| g.id != 0),
                    "{} {style:?}: eksik glif",
                    book_family_name(fam)
                );
            }
        }
    }

    #[test]
    fn bold_face_is_a_different_font() {
        let t = test_text();
        let regular = t.fonts().face(Family::Book(0), Style::default());
        let bold = t.fonts().face(
            Family::Book(0),
            Style {
                bold: true,
                ..Style::default()
            },
        );
        assert_ne!(regular, bold);
    }

    #[test]
    fn underlined_link_draws_extra_ink() {
        let t = test_text();
        let mut plain = Canvas::new(300, 80);
        let mut link = Canvas::new(300, 80);
        t.draw_plain(
            &mut plain,
            "bağlantı",
            Family::Book(0),
            Style::default(),
            10,
            50,
            36.0,
        );
        t.draw_plain(
            &mut link,
            "bağlantı",
            Family::Book(0),
            Style {
                link: true,
                ..Style::default()
            },
            10,
            50,
            36.0,
        );
        assert!(ink(&link) > ink(&plain));
    }
}
