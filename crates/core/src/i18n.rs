//! Kullanıcıya görünen tüm metinlerin katalogu.
//!
//! Arayüz metni yalnızca [`Msg`] üzerinden çizilebilir (bkz. `Font::draw_msg`),
//! bu yüzden koda gömülü bir arayüz metni derlenmez. Yeni bir dil eklemek,
//! [`tr`] yanına aynı `match`'i taşıyan bir fonksiyon eklemek demektir;
//! eksik çeviri derleme hatası verir.

/// Arayüz mesajları. Değişken taşıyanlar biçimlendirilerek gösterilir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    WindowTitle,
    TestTitle,
    TurkishSample,
    TestHint,
    NoTapYet,
    LastTap { x: u32, y: u32 },
    PageKeys { back: u32, forward: u32 },
    Refreshes { partial: u32, full: u32 },
    FontMissing,
    FontInvalid,
    WindowFailed,
    Crashed,
}

/// Türkçe metin.
pub fn tr(msg: &Msg) -> String {
    match msg {
        Msg::WindowTitle => "Legentia — Libra 2 simülatörü".into(),
        Msg::TestTitle => "Legentia test ekranı".into(),
        Msg::TurkishSample => "Türkçe: İı Iı Ğğ Üü Şş Öö Çç — “Ağaç gölgesinde çay.”".into(),
        Msg::TestHint => "Ekrana dokunun ya da sayfa tuşlarına basın.".into(),
        Msg::NoTapYet => "Henüz dokunma yok.".into(),
        Msg::LastTap { x, y } => format!("Son dokunma: x = {x}, y = {y}"),
        Msg::PageKeys { back, forward } => format!("Sayfa tuşları: geri {back}, ileri {forward}"),
        Msg::Refreshes { partial, full } => format!("Yenileme: kısmi {partial}, tam {full}"),
        Msg::FontMissing => "Yazı tipi bulunamadı. LEGENTIA_FONT ortam değişkeni bir .ttf dosyasını göstermeli (nix develop bunu ayarlar).".into(),
        Msg::FontInvalid => "Yazı tipi dosyası okunamadı ya da geçersiz.".into(),
        Msg::WindowFailed => "Simülatör penceresi açılamadı.".into(),
        Msg::Crashed => "Beklenmeyen bir hata oluştu ve uygulama kapandı. Ayrıntılar kayıt dosyasında.".into(),
    }
}

/// Kullanıcıya bir hata mesajını ekran dışında (terminalde) gösterir.
/// Cihazda ekrana çizilecek; şimdilik standart hataya yazar.
#[allow(clippy::print_stderr)]
pub fn show_error(msg: &Msg) {
    eprintln!("{}", tr(msg));
}

/// Panikte teknik ayrıntıyı log'a, Türkçe açıklamayı kullanıcıya verir.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
        show_error(&Msg::Crashed);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<Msg> {
        vec![
            Msg::WindowTitle,
            Msg::TestTitle,
            Msg::TurkishSample,
            Msg::TestHint,
            Msg::NoTapYet,
            Msg::LastTap { x: 1, y: 2 },
            Msg::PageKeys {
                back: 1,
                forward: 2,
            },
            Msg::Refreshes {
                partial: 1,
                full: 2,
            },
            Msg::FontMissing,
            Msg::FontInvalid,
            Msg::WindowFailed,
            Msg::Crashed,
        ]
    }

    #[test]
    fn every_message_has_turkish_text() {
        for m in all() {
            assert!(!tr(&m).trim().is_empty(), "boş çeviri: {m:?}");
        }
    }

    #[test]
    fn arguments_are_formatted() {
        assert_eq!(
            tr(&Msg::LastTap { x: 612, y: 840 }),
            "Son dokunma: x = 612, y = 840"
        );
    }

    #[test]
    fn sample_contains_turkish_letters() {
        let s = tr(&Msg::TurkishSample);
        for c in ['İ', 'ı', 'Ğ', 'ğ', 'Ü', 'ü', 'Ş', 'ş', 'Ö', 'ö', 'Ç', 'ç'] {
            assert!(s.contains(c), "eksik harf: {c}");
        }
    }
}
