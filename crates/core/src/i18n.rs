//! Kullanıcıya görünen tüm metinlerin katalogu.
//!
//! Arayüz metni yalnızca [`Msg`] üzerinden çizilebilir (bkz. `Font::draw_msg`).
//! Kitap içeriği (başlık, yazar, bölüm metni) arayüz metni değildir ve
//! crate içinden `Font::draw_content` ile çizilir. Yeni bir dil eklemek,
//! [`tr`] yanına aynı `match`'i taşıyan bir fonksiyon eklemek demektir;
//! eksik çeviri derleme hatası verir.

/// Arayüz mesajları. Değişken taşıyanlar biçimlendirilerek gösterilir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    WindowTitle,
    Contents,
    UntitledBook,
    UnknownAuthor,
    ChapterNumber(usize),
    EmptyChapter,
    ContentsFooter {
        page: usize,
        pages: usize,
    },
    ReadingFooter {
        chapter: usize,
        chapters: usize,
        page: usize,
        pages: usize,
    },
    TapToReturn,
    NoBookGiven,
    BookFileUnreadable,
    BookNotEpub,
    BookDamaged,
    BookTooLarge,
    BookUnsafe,
    FontMissing,
    WindowFailed,
    Crashed,
}

/// Türkçe metin.
pub fn tr(msg: &Msg) -> String {
    match msg {
        Msg::WindowTitle => "Legentia — Libra 2 simülatörü".into(),
        Msg::Contents => "İçindekiler".into(),
        Msg::UntitledBook => "Adsız kitap".into(),
        Msg::UnknownAuthor => "Yazarı bilinmiyor".into(),
        Msg::ChapterNumber(n) => format!("Bölüm {n}"),
        Msg::EmptyChapter => "Bu bölümde gösterilecek metin yok (yalnızca görsel olabilir).".into(),
        Msg::ContentsFooter { page, pages } => format!("Sayfa {page}/{pages}"),
        Msg::ReadingFooter { chapter, chapters, page, pages } => {
            format!("Bölüm {chapter}/{chapters} · Sayfa {page}/{pages}")
        }
        Msg::TapToReturn => "İçindekilere dönmek için ekrana dokunun.".into(),
        Msg::NoBookGiven => "Kitap dosyası verilmedi. Kullanım: cargo run -p legentia-sim -- kitap.epub".into(),
        Msg::BookFileUnreadable => "Kitap dosyası açılamadı. Dosyanın var olduğundan ve okunabildiğinden emin olun.".into(),
        Msg::BookNotEpub => "Bu dosya bir EPUB kitabı değil ya da bozuk.".into(),
        Msg::BookDamaged => "Kitap dosyası hasarlı: gerekli bir bölümü eksik ya da okunamıyor.".into(),
        Msg::BookTooLarge => "Kitabın bir bölümü güvenli boyut sınırını aşıyor; açılmadı.".into(),
        Msg::BookUnsafe => "Kitap, paketin dışına çıkan güvensiz bir dosya yolu içeriyor; açılmadı.".into(),
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
            Msg::Contents,
            Msg::UntitledBook,
            Msg::UnknownAuthor,
            Msg::ChapterNumber(3),
            Msg::EmptyChapter,
            Msg::ContentsFooter { page: 1, pages: 2 },
            Msg::ReadingFooter {
                chapter: 1,
                chapters: 2,
                page: 3,
                pages: 4,
            },
            Msg::TapToReturn,
            Msg::NoBookGiven,
            Msg::BookFileUnreadable,
            Msg::BookNotEpub,
            Msg::BookDamaged,
            Msg::BookTooLarge,
            Msg::BookUnsafe,
            Msg::FontMissing,
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
        let footer = Msg::ReadingFooter {
            chapter: 2,
            chapters: 12,
            page: 3,
            pages: 9,
        };
        assert_eq!(tr(&footer), "Bölüm 2/12 · Sayfa 3/9");
        assert_eq!(tr(&Msg::ChapterNumber(7)), "Bölüm 7");
    }
}
