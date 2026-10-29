use serde::{Deserialize, Serialize};

use crate::i18n::{Msg, tr};
use crate::text::{book_family_count, book_family_name};

/// Ayar adımları. Değerler listeden seçilir; artırıp azaltırken kayan nokta hatası birikmez.
pub const FONT_SIZES: &[f32] = &[28.0, 32.0, 36.0, 40.0, 44.0, 48.0, 54.0, 60.0, 68.0, 76.0];
pub const LINE_SPACINGS: &[f32] = &[1.2, 1.3, 1.4, 1.5, 1.6, 1.8, 2.0];
pub const MARGINS: &[i32] = &[30, 50, 70, 100, 130, 160];
/// Otomatik sayfa çevirme aralıkları (saniye); 0 = okuma hızına göre.
pub const AUTO_TURNS: &[u32] = &[0, 10, 15, 20, 30, 45, 60, 90, 120];
/// PDF yakınlaştırma adımları (yüzde).
pub const PDF_ZOOMS: &[u32] = &[100, 125, 150, 200];

/// PDF sayfa görüntüsünün ekrana yerleşimi. Ekrana sığmayan kısım birden çok
/// ekrana bölünür; sayfa tuşları ekran ekran ilerler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PdfView {
    /// Sayfanın tamamı tek ekranda.
    #[default]
    Page,
    /// Sayfa genişliği ekrana sığar; boyu ekranlara bölünür.
    Width,
    /// Sütunlar sırayla, her biri genişliğe sığdırılarak.
    Columns,
    /// Webtoon: sayfalar genişliğe sığdırılıp kesintisiz alt alta; ekran
    /// sayfa sonunu geçerse sonraki sayfanın başı altında görünür.
    Strip,
}

/// Okurken ekrana dokunmanın anlamı.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TapZones {
    /// Sol üçte bir geri, orta menü, sağ üçte bir ileri.
    #[default]
    Standard,
    /// Sol ileri, sağ geri.
    Reversed,
    /// Tek elle: sol kenar şeridi geri, üst şerit menü, geri kalan her yer ileri.
    OneHand,
}

/// Okurken ekranın yönü. Yalnızca okuma ekranı döner; ayarlar, listeler ve
/// formlar dikeyde kalır.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Orientation {
    /// Cihazın ivmeölçerine göre.
    #[default]
    Auto,
    Portrait,
    Landscape,
}

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
    pub pdf_view: PdfView,
    /// Yakınlaştırma (yüzde); `pdf_view`'in sığdırdığı boyutun bu katı.
    pub pdf_zoom: u32,
    /// PDF'in metnini sayfa görüntüsü yerine EPUB gibi yeniden diz.
    pub pdf_reflow: bool,
    /// Yeniden akıtırken metin katmanı olmayan (taranmış) sayfaları OCR ile oku.
    pub ocr: bool,
    /// İnternet özellikleri (kataloglar, indirme). Kapalıyken hiç bağlantı kurulmaz.
    pub network: bool,
    /// Kelime cihazdaki sözlüklerde yoksa TDK'ya sor.
    pub online_tdk: bool,
    /// Kelime cihazdaki sözlüklerde yoksa Vikisözlük'e sor.
    pub online_wiktionary: bool,
    pub tap_zones: TapZones,
    /// Kitap metninin kalınlaştırılması: 0 (yok) – 3.
    pub weight: u8,
    pub orientation: Orientation,
    /// Yatayda EPUB iki sütun (açık kitap gibi); değilse tek geniş sütun.
    pub two_columns: bool,
    /// Sayfa tuşlarının anlamı ters: üst tuş ileri, alt tuş geri.
    pub swap_keys: bool,
    /// Otomatik sayfa çevirme aralığı (saniye; 0 = okuma hızına göre).
    pub auto_turn: u32,
    /// Sayfa yönü sağdan sola (manga): dokunma, kaydırma, yan yana sayfalar
    /// ve yakınlaştırılmış sayfanın parçaları ters sırada. Kitaba özel.
    pub rtl: bool,
    /// Arayüz dili (genel).
    pub language: crate::i18n::Language,
    /// Kütüphane araç çubuğunun sıralaması, süzgeci ve görünümü.
    pub library_sort: super::shelf::Sort,
    pub library_filter: super::shelf::Filter,
    pub library_view: super::shelf::LibraryView,
    /// Yeni vurgunun biçimi: son seçilen.
    pub highlight: super::annotations::Mark,
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
            pdf_view: PdfView::Page,
            pdf_zoom: 100,
            pdf_reflow: false,
            ocr: false,
            network: true,
            online_tdk: true,
            online_wiktionary: false,
            tap_zones: TapZones::Standard,
            weight: 0,
            orientation: Orientation::Auto,
            two_columns: true,
            swap_keys: false,
            auto_turn: 30,
            rtl: false,
            language: Default::default(),
            library_sort: Default::default(),
            library_filter: Default::default(),
            library_view: Default::default(),
            highlight: Default::default(),
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
    PdfView,
    PdfZoom,
    PdfCrop,
    PdfReflow,
    Ocr,
    Network,
    OnlineTdk,
    OnlineWiktionary,
    TapZones,
    Weight,
    Orientation,
    TwoColumns,
    PageKeys,
    AutoTurn,
    Rtl,
    Language,
}

/// Ayarlar ekranının sayfaları: yazı; PDF ve internet; okuma ve ekran. Hepsi
/// tek ekrana sığmaz (alttaki Tamam/Vazgeç düğmelerine taşar).
pub const FIELD_PAGES: [&[Field]; 3] = [
    &[
        Field::Family,
        Field::Size,
        Field::Weight,
        Field::LineSpacing,
        Field::Margin,
        Field::Align,
        Field::Hyphenate,
    ],
    &[
        Field::PdfView,
        Field::PdfZoom,
        Field::PdfCrop,
        Field::PdfReflow,
        Field::Ocr,
        Field::Network,
        Field::OnlineTdk,
        Field::OnlineWiktionary,
    ],
    &[
        Field::TapZones,
        Field::Rtl,
        Field::PageKeys,
        Field::AutoTurn,
        Field::Night,
        Field::Orientation,
        Field::TwoColumns,
        Field::Language,
    ],
];

/// Ayar sayfalarının başlıkları ve bir sonrakine geçiş düğmesinin etiketi.
pub const PAGE_TITLES: [Msg; 3] = [Msg::TextSettings, Msg::PdfSettings, Msg::ReadingSettings];

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
            Field::PdfView => Msg::SettingPdfView,
            Field::PdfZoom => Msg::SettingPdfZoom,
            Field::PdfCrop => Msg::SettingPdfCrop,
            Field::PdfReflow => Msg::SettingPdfReflow,
            Field::Ocr => Msg::SettingOcr,
            Field::Network => Msg::SettingNetwork,
            Field::OnlineTdk => Msg::SettingTdk,
            Field::OnlineWiktionary => Msg::SettingWiktionary,
            Field::TapZones => Msg::SettingTapZones,
            Field::Weight => Msg::SettingWeight,
            Field::Orientation => Msg::SettingOrientation,
            Field::TwoColumns => Msg::SettingTwoColumns,
            Field::Rtl => Msg::SettingRtl,
            Field::Language => Msg::SettingLanguage,
            Field::PageKeys => Msg::SettingPageKeys,
            Field::AutoTurn => Msg::SettingAutoTurn,
        }
    }
}

impl Settings {
    /// Kitaba özel alanları (yazı ve PDF görünümü) `book`'tan, genel alanları
    /// (gece modu, dokunma düzeni, internet…) `self`'ten alır.
    pub fn with_book_fields(&self, book: &Settings) -> Settings {
        Settings {
            family: book.family,
            font_px: book.font_px,
            line_spacing: book.line_spacing,
            margin: book.margin,
            justify: book.justify,
            hyphenate: book.hyphenate,
            weight: book.weight,
            pdf_crop: book.pdf_crop,
            pdf_view: book.pdf_view,
            pdf_zoom: book.pdf_zoom,
            pdf_reflow: book.pdf_reflow,
            ocr: book.ocr,
            rtl: book.rtl,
            ..self.clone()
        }
    }

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
            Field::PdfView => {
                const VIEWS: [PdfView; 4] = [
                    PdfView::Page,
                    PdfView::Width,
                    PdfView::Columns,
                    PdfView::Strip,
                ];
                let n = VIEWS.len();
                let i = VIEWS.iter().position(|v| *v == s.pdf_view).unwrap_or(0);
                s.pdf_view = VIEWS[if up { (i + 1) % n } else { (i + n - 1) % n }];
            }
            Field::PdfZoom => {
                s.pdf_zoom = PDF_ZOOMS[step(
                    PDF_ZOOMS.len(),
                    |i| PDF_ZOOMS[i] as f32,
                    s.pdf_zoom as f32,
                    up,
                )]
            }
            Field::PdfCrop => s.pdf_crop = !s.pdf_crop,
            Field::Rtl => s.rtl = !s.rtl,
            Field::Language => {
                use crate::i18n::Language;
                s.language = match s.language {
                    Language::Turkish => Language::English,
                    Language::English => Language::Turkish,
                }
            }
            Field::PdfReflow => s.pdf_reflow = !s.pdf_reflow,
            Field::Ocr => s.ocr = !s.ocr,
            Field::Network => s.network = !s.network,
            Field::OnlineTdk => s.online_tdk = !s.online_tdk,
            Field::OnlineWiktionary => s.online_wiktionary = !s.online_wiktionary,
            Field::Weight => {
                s.weight = if up {
                    (s.weight + 1).min(3)
                } else {
                    s.weight.saturating_sub(1)
                }
            }
            Field::Orientation => {
                const ALL: [Orientation; 3] = [
                    Orientation::Auto,
                    Orientation::Portrait,
                    Orientation::Landscape,
                ];
                let i = ALL.iter().position(|o| *o == s.orientation).unwrap_or(0);
                s.orientation = ALL[if up { (i + 1) % 3 } else { (i + 2) % 3 }];
            }
            Field::TwoColumns => s.two_columns = !s.two_columns,
            Field::PageKeys => s.swap_keys = !s.swap_keys,
            Field::AutoTurn => {
                let i = AUTO_TURNS
                    .iter()
                    .position(|v| *v == s.auto_turn)
                    .unwrap_or(4);
                let i = if up {
                    (i + 1).min(AUTO_TURNS.len() - 1)
                } else {
                    i.saturating_sub(1)
                };
                s.auto_turn = AUTO_TURNS[i];
            }
            Field::TapZones => {
                const ZONES: [TapZones; 3] =
                    [TapZones::Standard, TapZones::Reversed, TapZones::OneHand];
                let i = ZONES.iter().position(|z| *z == s.tap_zones).unwrap_or(0);
                s.tap_zones = ZONES[if up { (i + 1) % 3 } else { (i + 2) % 3 }];
            }
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
            Field::PdfView => tr(match self.pdf_view {
                PdfView::Page => &Msg::PdfViewPage,
                PdfView::Width => &Msg::PdfViewWidth,
                PdfView::Columns => &Msg::PdfViewColumns,
                PdfView::Strip => &Msg::PdfViewStrip,
            }),
            Field::Language => tr(&Msg::LanguageName(self.language)),
            Field::Rtl => tr(if self.rtl {
                &Msg::RightToLeft
            } else {
                &Msg::LeftToRight
            }),
            Field::PdfZoom => tr(&Msg::Percent(self.pdf_zoom)),
            Field::PdfCrop => tr(if self.pdf_crop { &Msg::On } else { &Msg::Off }),
            Field::PdfReflow => tr(if self.pdf_reflow { &Msg::On } else { &Msg::Off }),
            Field::Ocr => tr(if self.ocr { &Msg::On } else { &Msg::Off }),
            Field::Network => tr(if self.network { &Msg::On } else { &Msg::Off }),
            Field::OnlineTdk => tr(if self.online_tdk { &Msg::On } else { &Msg::Off }),
            Field::OnlineWiktionary => tr(if self.online_wiktionary {
                &Msg::On
            } else {
                &Msg::Off
            }),
            Field::Weight => tr(&Msg::Weight(self.weight)),
            Field::Orientation => tr(match self.orientation {
                Orientation::Auto => &Msg::OrientationAuto,
                Orientation::Portrait => &Msg::OrientationPortrait,
                Orientation::Landscape => &Msg::OrientationLandscape,
            }),
            Field::AutoTurn => tr(&match self.auto_turn {
                0 => Msg::AutoTurnPace,
                secs => Msg::Seconds(secs),
            }),
            Field::PageKeys => tr(if self.swap_keys {
                &Msg::KeysUpForward
            } else {
                &Msg::KeysUpBack
            }),
            Field::TwoColumns => tr(if self.two_columns {
                &Msg::ColumnsTwo
            } else {
                &Msg::ColumnsOne
            }),
            Field::TapZones => tr(match self.tap_zones {
                TapZones::Standard => &Msg::TapStandard,
                TapZones::Reversed => &Msg::TapReversed,
                TapZones::OneHand => &Msg::TapOneHand,
            }),
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
        assert_eq!(s.value_text(Field::PdfView), "Sayfaya sığdır");
        assert_eq!(
            s.step(Field::PdfZoom, true).value_text(Field::PdfZoom),
            "%125"
        );
    }

    #[test]
    fn pdf_view_cycles_both_ways() {
        let s = Settings::default();
        assert_eq!(s.step(Field::PdfView, true).pdf_view, PdfView::Width);
        assert_eq!(s.step(Field::PdfView, false).pdf_view, PdfView::Strip);
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
