use serde::{Deserialize, Serialize};

use crate::i18n::{Msg, tr};
use crate::text::{book_family_count, book_family_name};

/// Ayar adımları. Değerler listeden seçilir; artırıp azaltırken kayan nokta hatası birikmez.
pub const FONT_SIZES: &[f32] = &[28.0, 32.0, 36.0, 40.0, 44.0, 48.0, 54.0, 60.0, 68.0, 76.0];
pub const LINE_SPACINGS: &[f32] = &[1.2, 1.3, 1.4, 1.5, 1.6, 1.8, 2.0];
pub const MARGINS: &[i32] = &[30, 50, 70, 100, 130, 160];

/// Okuma ayarları. Değişince bölüm yeniden dizilir, okuma konumu korunur.
/// Kayıt dosyasında eksik alanlar varsayılanla doldurulur.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// `text::book_family_name` sırası.
    pub family: usize,
    /// Gövde metni boyutu (piksel; 300 ppi'de 40 px ≈ 9,6 punto).
    pub font_px: f32,
    pub line_spacing: f32,
    /// Sol ve sağ kenar boşluğu (piksel).
    pub margin: i32,
    pub justify: bool,
    pub hyphenate: bool,
    /// Ters renk: siyah zemin, beyaz yazı.
    pub night: bool,
    /// PDF sayfalarının beyaz kenarlarını kırpıp içeriği ekrana büyüt.
    pub pdf_crop: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            family: 0,
            font_px: 40.0,
            line_spacing: 1.4,
            margin: 70,
            justify: true,
            hyphenate: true,
            night: false,
            pdf_crop: true,
        }
    }
}

/// Ayarlar ekranındaki satırlar, gösterim sırasıyla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Family,
    Size,
    LineSpacing,
    Margin,
    Align,
    Hyphenate,
    Night,
    PdfCrop,
}

pub const FIELDS: [Field; 8] = [
    Field::Family,
    Field::Size,
    Field::LineSpacing,
    Field::Margin,
    Field::Align,
    Field::Hyphenate,
    Field::Night,
    Field::PdfCrop,
];

impl Field {
    pub fn label(self) -> Msg {
        match self {
            Field::Family => Msg::SettingFont,
            Field::Size => Msg::SettingSize,
            Field::LineSpacing => Msg::SettingLineSpacing,
            Field::Margin => Msg::SettingMargin,
            Field::Align => Msg::SettingAlign,
            Field::Hyphenate => Msg::SettingHyphenate,
            Field::Night => Msg::SettingNight,
            Field::PdfCrop => Msg::SettingPdfCrop,
        }
    }
}

impl Settings {
    /// Bir alanı bir adım ileri (`up`) ya da geri alır. Açık/kapalı alanlarda iki yön de değiştirir.
    pub fn step(&self, field: Field, up: bool) -> Settings {
        let mut s = self.clone();
        match field {
            Field::Family => {
                let n = book_family_count();
                s.family = if up {
                    (s.family + 1) % n
                } else {
                    (s.family + n - 1) % n
                };
            }
            Field::Size => {
                s.font_px = FONT_SIZES[step(FONT_SIZES.len(), |i| FONT_SIZES[i], s.font_px, up)]
            }
            Field::LineSpacing => {
                s.line_spacing = LINE_SPACINGS[step(
                    LINE_SPACINGS.len(),
                    |i| LINE_SPACINGS[i],
                    s.line_spacing,
                    up,
                )]
            }
            Field::Margin => {
                s.margin = MARGINS[step(MARGINS.len(), |i| MARGINS[i] as f32, s.margin as f32, up)]
            }
            Field::Align => s.justify = !s.justify,
            Field::Hyphenate => s.hyphenate = !s.hyphenate,
            Field::Night => s.night = !s.night,
            Field::PdfCrop => s.pdf_crop = !s.pdf_crop,
        }
        s
    }

    /// Alanın Türkçe gösterimi: "9,6 punto", "1,4", "6 mm", "İki yana", "Açık"…
    pub fn value_text(&self, field: Field) -> String {
        match field {
            Field::Family => book_family_name(self.family).to_string(),
            Field::Size => tr(&Msg::Points {
                tenths: (self.font_px * 72.0 / 300.0 * 10.0).round() as u32,
            }),
            Field::LineSpacing => tr(&Msg::Decimal {
                tenths: (self.line_spacing * 10.0).round() as u32,
            }),
            Field::Margin => tr(&Msg::Millimeters(
                (self.margin as f32 * 25.4 / 300.0).round() as u32,
            )),
            Field::Align => tr(if self.justify {
                &Msg::AlignJustify
            } else {
                &Msg::AlignLeft
            }),
            Field::Hyphenate => tr(if self.hyphenate { &Msg::On } else { &Msg::Off }),
            Field::Night => tr(if self.night { &Msg::On } else { &Msg::Off }),
            Field::PdfCrop => tr(if self.pdf_crop { &Msg::On } else { &Msg::Off }),
        }
    }
}

/// `current`'a en yakın adımın bir ötesinin dizini; uçlarda durur.
fn step(len: usize, value: impl Fn(usize) -> f32, current: f32, up: bool) -> usize {
    let nearest = (0..len)
        .min_by(|&a, &b| {
            (value(a) - current)
                .abs()
                .total_cmp(&(value(b) - current).abs())
        })
        .unwrap_or(0);
    if up {
        (nearest + 1).min(len - 1)
    } else {
        nearest.saturating_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_stay_on_the_list_without_drift() {
        let mut s = Settings::default();
        for _ in 0..20 {
            s = s.step(Field::LineSpacing, true);
        }
        assert_eq!(s.line_spacing, 2.0);
        for _ in 0..3 {
            s = s.step(Field::LineSpacing, false);
        }
        assert_eq!(s.line_spacing, 1.5);
        assert_eq!(Settings::default().step(Field::Size, false).font_px, 36.0);
        assert_eq!(Settings::default().step(Field::Margin, true).margin, 100);
    }

    #[test]
    fn values_are_shown_in_turkish_format() {
        let s = Settings::default();
        assert_eq!(s.value_text(Field::Size), "9,6 punto");
        assert_eq!(s.value_text(Field::LineSpacing), "1,4");
        assert_eq!(s.value_text(Field::Margin), "6 mm");
        assert_eq!(s.value_text(Field::Align), "İki yana");
        assert_eq!(s.value_text(Field::Night), "Kapalı");
        assert_eq!(s.step(Field::Night, true).value_text(Field::Night), "Açık");
    }

    #[test]
    fn family_wraps_around_both_ways() {
        let s = Settings::default();
        assert_eq!(s.step(Field::Family, false).family, book_family_count() - 1);
        assert_eq!(s.step(Field::Family, true).step(Field::Family, false), s);
    }

    #[test]
    fn missing_fields_in_saved_file_use_defaults() {
        let s: Settings = serde_json::from_str(r#"{"font_px": 48.0}"#).unwrap();
        assert_eq!(s.font_px, 48.0);
        assert_eq!(s.line_spacing, Settings::default().line_spacing);
    }
}
