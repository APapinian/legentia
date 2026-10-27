//! Kullanıcıya görünen tüm metinlerin katalogu.
//!
//! Arayüz metni yalnızca [`Msg`] üzerinden çizilebilir (bkz. `Text::draw_msg`).
//! Kitap içeriği (başlık, yazar, bölüm metni) ve font adları arayüz metni
//! değildir; crate içinden `Text::draw_plain` ile çizilir. Yeni bir dil eklemek,
//! [`tr`] yanına aynı `match`'i taşıyan bir fonksiyon eklemek demektir;
//! eksik çeviri derleme hatası verir.

/// Arayüz mesajları. Değişken taşıyanlar biçimlendirilerek gösterilir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    WindowTitle,
    Usage,
    // Kütüphane
    Library,
    BookCount(usize),
    LibraryEmpty,
    Unreadable,
    Percent(u32),
    ListFooter {
        page: usize,
        pages: usize,
    },
    Back,
    // Kitap
    Contents,
    UnknownAuthor,
    ChapterNumber(usize),
    EmptyChapter,
    ReadingFooter {
        chapter: usize,
        chapters: usize,
        page: usize,
        pages: usize,
        percent: u32,
    },
    PdfFooter {
        page: usize,
        pages: usize,
        percent: u32,
    },
    PdfReflowFooter {
        page: usize,
        pages: usize,
        screen: usize,
        screens: usize,
        percent: u32,
    },
    PageNumber(usize),
    PdfDocument,
    // Menü
    AddBookmark,
    RemoveBookmark,
    Bookmarks,
    NoBookmarks,
    TextSettings,
    PdfSettings,
    PdfSettingsNext,
    TextSettingsBack,
    BackToReading,
    SaveSearchable,
    SearchableSaved {
        pages: usize,
        name: String,
    },
    NothingToOcr,
    SearchableFailed,
    // Yazı ayarları
    SettingsHint,
    SettingFont,
    SettingSize,
    SettingLineSpacing,
    SettingMargin,
    SettingAlign,
    SettingHyphenate,
    SettingNight,
    SettingPdfCrop,
    SettingPdfReflow,
    SettingOcr,
    Points {
        tenths: u32,
    },
    Decimal {
        tenths: u32,
    },
    Millimeters(u32),
    AlignJustify,
    AlignLeft,
    On,
    Off,
    PreviewSample,
    Ok,
    Cancel,
    // Hatalar
    TapToReturn,
    BookFileUnreadable,
    BookNotEpub,
    BookDamaged,
    BookTooLarge,
    BookUnsafe,
    PdfEncrypted,
    PdfDamaged,
    PdfPageFailed,
    FontMissing,
    WindowFailed,
    Crashed,
}

/// Ondalık sayı, Türkçe virgülle: 14 → "1,4".
fn decimal(tenths: u32) -> String {
    format!("{},{}", tenths / 10, tenths % 10)
}

/// Türkçe metin.
pub fn tr(msg: &Msg) -> String {
    match msg {
        Msg::WindowTitle => "Legentia — Libra 2 simülatörü".into(),
        Msg::Usage => "Kullanım: legentia-sim <kitap klasörü ya da kitap.epub>".into(),
        Msg::Library => "Kütüphane".into(),
        Msg::BookCount(n) => format!("{n} kitap"),
        Msg::LibraryEmpty => "Kütüphanede kitap yok. EPUB dosyalarını USB ile cihaza kopyalayın.".into(),
        Msg::Unreadable => "Açılamıyor".into(),
        // Türkçede yüzde işareti sayıdan önce gelir.
        Msg::Percent(p) => format!("%{p}"),
        Msg::ListFooter { page, pages } => format!("Sayfa {page}/{pages}"),
        Msg::Back => "Geri".into(),
        Msg::Contents => "İçindekiler".into(),
        Msg::UnknownAuthor => "Yazarı bilinmiyor".into(),
        Msg::ChapterNumber(n) => format!("Bölüm {n}"),
        Msg::EmptyChapter => "Bu bölümde gösterilecek metin yok (yalnızca görsel olabilir).".into(),
        Msg::ReadingFooter { chapter, chapters, page, pages, percent } => {
            format!("Bölüm {chapter}/{chapters} · Sayfa {page}/{pages} · %{percent}")
        }
        Msg::PdfFooter { page, pages, percent } => format!("Sayfa {page}/{pages} · %{percent}"),
        Msg::PdfReflowFooter { page, pages, screen, screens, percent } => {
            format!("Sayfa {page}/{pages} ({screen}/{screens}) · %{percent}")
        }
        Msg::PageNumber(n) => format!("Sayfa {n}"),
        Msg::PdfDocument => "PDF belgesi".into(),
        Msg::AddBookmark => "Bu sayfaya yer imi koy".into(),
        Msg::RemoveBookmark => "Bu sayfadaki yer imini kaldır".into(),
        Msg::Bookmarks => "Yer imleri".into(),
        Msg::NoBookmarks => "Henüz yer imi yok. Okurken ekranın ortasına dokunup menüden ekleyebilirsiniz.".into(),
        Msg::TextSettings => "Yazı ayarları".into(),
        Msg::PdfSettings => "PDF ayarları".into(),
        Msg::PdfSettingsNext => "PDF ayarları →".into(),
        Msg::TextSettingsBack => "← Yazı ayarları".into(),
        Msg::BackToReading => "Okumaya dön".into(),
        Msg::SaveSearchable => "Aranabilir PDF olarak kaydet".into(),
        Msg::SearchableSaved { pages, name } => {
            format!("Aranabilir PDF kaydedildi: “{name}”. {pages} sayfaya metin katmanı eklendi; orijinal dosya değişmedi.")
        }
        Msg::NothingToOcr => "Bu PDF'in bütün sayfalarında zaten metin var; aranabilir kopyaya gerek yok.".into(),
        Msg::SearchableFailed => "Aranabilir PDF kaydedilemedi. Ayrıntılar kayıt dosyasında.".into(),
        Msg::SettingsHint => "Değişiklikler “Tamam”a dokununca uygulanır.".into(),
        Msg::SettingFont => "Yazı tipi".into(),
        Msg::SettingSize => "Yazı boyutu".into(),
        Msg::SettingLineSpacing => "Satır aralığı".into(),
        Msg::SettingMargin => "Kenar boşluğu".into(),
        Msg::SettingAlign => "Hizalama".into(),
        Msg::SettingHyphenate => "Heceleme".into(),
        Msg::SettingNight => "Gece modu (ters renk)".into(),
        Msg::SettingPdfCrop => "PDF kenarlarını kırp".into(),
        Msg::SettingPdfReflow => "PDF metnini yeniden akıt".into(),
        Msg::SettingOcr => "Taranmış sayfaları OCR ile oku".into(),
        Msg::Points { tenths } => format!("{} punto", decimal(*tenths)),
        Msg::Decimal { tenths } => decimal(*tenths),
        Msg::Millimeters(mm) => format!("{mm} mm"),
        Msg::AlignJustify => "İki yana".into(),
        Msg::AlignLeft => "Sola".into(),
        Msg::On => "Açık".into(),
        Msg::Off => "Kapalı".into(),
        Msg::PreviewSample => "Ağaçların gölgesinde çayımızı içerken ışık yaprakların arasından süzülüyordu.".into(),
        Msg::Ok => "Tamam".into(),
        Msg::Cancel => "Vazgeç".into(),
        Msg::TapToReturn => "Dönmek için ekrana dokunun.".into(),
        Msg::BookFileUnreadable => "Kitap dosyası açılamadı. Dosyanın var olduğundan ve okunabildiğinden emin olun.".into(),
        Msg::BookNotEpub => "Bu dosya bir EPUB kitabı değil ya da bozuk.".into(),
        Msg::BookDamaged => "Kitap dosyası hasarlı: gerekli bir bölümü eksik ya da okunamıyor.".into(),
        Msg::BookTooLarge => "Kitabın bir bölümü güvenli boyut sınırını aşıyor; açılmadı.".into(),
        Msg::BookUnsafe => "Kitap, paketin dışına çıkan güvensiz bir dosya yolu içeriyor; açılmadı.".into(),
        Msg::PdfEncrypted => "Bu PDF parolayla korunuyor; şimdilik açılamıyor.".into(),
        Msg::PdfDamaged => "PDF dosyası hasarlı ya da desteklenmeyen bir biçimde.".into(),
        Msg::PdfPageFailed => "Bu sayfa çizilemedi. Diğer sayfalara geçebilirsiniz.".into(),
        Msg::FontMissing => "Yazı tipleri bulunamadı ya da okunamadı. LEGENTIA_FONTS ortam değişkeni font klasörünü göstermeli (nix develop bunu ayarlar).".into(),
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

/// Panikte teknik ayrıntıyı log'a yazar. Kullanıcıya mesaj göstermez: yakalanıp
/// toparlanılan panikler de (ör. bozuk PDF sayfası) buradan geçer. Uygulamanın
/// gerçekten kapandığını platform katmanı bildirir (`Msg::Crashed`).
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<Msg> {
        use Msg::*;
        vec![
            WindowTitle,
            Usage,
            Library,
            BookCount(3),
            LibraryEmpty,
            Unreadable,
            Percent(45),
            ListFooter { page: 1, pages: 2 },
            Back,
            Contents,
            UnknownAuthor,
            ChapterNumber(3),
            EmptyChapter,
            ReadingFooter {
                chapter: 1,
                chapters: 2,
                page: 3,
                pages: 4,
                percent: 5,
            },
            PdfFooter {
                page: 1,
                pages: 2,
                percent: 50,
            },
            PdfReflowFooter {
                page: 1,
                pages: 2,
                screen: 1,
                screens: 3,
                percent: 10,
            },
            PageNumber(4),
            PdfDocument,
            AddBookmark,
            RemoveBookmark,
            Bookmarks,
            NoBookmarks,
            TextSettings,
            PdfSettings,
            PdfSettingsNext,
            TextSettingsBack,
            BackToReading,
            SaveSearchable,
            SearchableSaved {
                pages: 3,
                name: "kitap - aranabilir.pdf".into(),
            },
            NothingToOcr,
            SearchableFailed,
            SettingsHint,
            SettingFont,
            SettingSize,
            SettingLineSpacing,
            SettingMargin,
            SettingAlign,
            SettingHyphenate,
            SettingNight,
            SettingPdfCrop,
            SettingPdfReflow,
            SettingOcr,
            Points { tenths: 96 },
            Decimal { tenths: 14 },
            Millimeters(6),
            AlignJustify,
            AlignLeft,
            On,
            Off,
            PreviewSample,
            Ok,
            Cancel,
            TapToReturn,
            BookFileUnreadable,
            BookNotEpub,
            BookDamaged,
            BookTooLarge,
            BookUnsafe,
            PdfEncrypted,
            PdfDamaged,
            PdfPageFailed,
            FontMissing,
            WindowFailed,
            Crashed,
        ]
    }

    #[test]
    fn every_message_has_turkish_text() {
        for m in all() {
            assert!(!tr(&m).trim().is_empty(), "boş çeviri: {m:?}");
        }
    }

    #[test]
    fn numbers_use_turkish_conventions() {
        let footer = Msg::ReadingFooter {
            chapter: 2,
            chapters: 12,
            page: 3,
            pages: 9,
            percent: 17,
        };
        assert_eq!(tr(&footer), "Bölüm 2/12 · Sayfa 3/9 · %17");
        assert_eq!(tr(&Msg::Percent(45)), "%45");
        assert_eq!(tr(&Msg::Decimal { tenths: 14 }), "1,4");
        assert_eq!(tr(&Msg::Points { tenths: 96 }), "9,6 punto");
    }
}
