use std::path::{Path, PathBuf};

use super::screens::{BOOKMARKS, CONTENTS, LIBRARY, MenuItem};
use super::settings::{FIELD_PAGES, Field};
use super::*;
use crate::LIBRA2_HEIGHT;
use crate::LIBRA2_WIDTH;
use crate::epub::testutil::*;
use crate::event::PageKey;
use crate::storage::temp_store;
use crate::text::test_text;

const W: u32 = LIBRA2_WIDTH;

/// Kitapları geçici bir kütüphane klasörüne yazar; (uygulama, veri klasörü, kütüphane klasörü).
fn setup(books: &[(&str, Vec<u8>)]) -> (App, PathBuf, PathBuf) {
    let store = temp_store("app");
    let data = store.dir().to_path_buf();
    let root = data.join("kitaplar");
    std::fs::create_dir_all(&root).unwrap();
    for (name, bytes) in books {
        std::fs::write(root.join(name), bytes).unwrap();
    }
    let mut app = App::new(test_text(), W, store, &root);
    no_online_dictionaries(&mut app);
    (app, data, root)
}

/// Testler internete çıkmaz: çevrimiçi sözlük kapalı, adresleri kapalı bir porta
/// yönlenmiş. Çevrimiçi sözlük testleri kendi taklit sunucusunu verir.
fn no_online_dictionaries(app: &mut App) {
    for s in [&mut app.settings, &mut app.defaults] {
        s.online_tdk = false;
        s.online_wiktionary = false;
    }
    app.endpoints = super::lookup::Endpoints {
        tdk: "http://127.0.0.1:1/gts".into(),
        wiktionary: "http://127.0.0.1:1/api.php".into(),
        wikipedia: "http://127.0.0.1:1".into(),
        deepl: Some("http://127.0.0.1:1".into()),
        releases: "http://127.0.0.1:1/releases".into(),
    };
}

fn restart(data: &Path, root: &Path) -> App {
    let mut app = App::new(test_text(), W, Store::new(data), root);
    no_online_dictionaries(&mut app);
    app
}

fn long_book() -> Vec<u8> {
    let para = "<p>Uzun bir paragraf: ağaçlar, gölgeler, ışık ve çay; kardeşlerimizden biri İstanbul'dan geldi. </p>".repeat(300);
    epub_with_chapter(&format!("<html><body><h1>Başlık</h1>{para}</body></html>"))
}

fn tap(app: &mut App, (x, y): (u32, u32)) -> Option<Refresh> {
    app.handle(Event::Tap { x, y })
}

fn tap_row(app: &mut App, list: &super::ui::List, row: usize) -> Option<Refresh> {
    tap(app, list.row_rect(W, row).center())
}

fn key(app: &mut App, forward: bool) -> Option<Refresh> {
    app.handle(Event::Key(if forward {
        PageKey::Forward
    } else {
        PageKey::Back
    }))
}

fn view_name(app: &App) -> &'static str {
    match app.view {
        View::Reading => "okuma",
        View::Annotation { .. } => "vurgu",
        View::Footnote => "dipnot",
        View::Form(_) => "form",
        View::SearchResults { .. } => "arama sonuçları",
        View::Contents { .. } => "içindekiler",
        _ => "başka",
    }
}

fn middle(app: &mut App) -> Option<Refresh> {
    tap(app, (W / 2, LIBRA2_HEIGHT / 2))
}

fn menu(app: &mut App, item: MenuItem) -> Option<Refresh> {
    middle(app);
    assert!(
        matches!(app.view, View::Menu),
        "menü açılmadı: {}",
        view_name(app)
    );
    let index = app.menu_items().iter().position(|m| *m == item).unwrap();
    tap(app, app.menu_rect(index).center())
}

/// Ayarlar ekranında bir alanın "+" düğmesine basar; gerekirse önce o alanın sayfasına geçer.
fn press_plus(app: &mut App, field: Field) {
    let (page, row) = FIELD_PAGES
        .iter()
        .enumerate()
        .find_map(|(p, fields)| fields.iter().position(|f| *f == field).map(|r| (p, r)))
        .expect("alan bir ayar sayfasında olmalı");
    while !matches!(app.view, View::Settings { page: p, .. } if p == page) {
        let at = app.settings_page_button().center();
        tap(app, at);
    }
    let at = app.settings_buttons(row).1.center();
    tap(app, at);
}

/// Tek bölümlük uzun kitabı açıp okumaya başlar.
fn reading_long_book() -> (App, PathBuf, PathBuf) {
    let (mut app, data, root) = setup(&[("uzun.epub", long_book())]);
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    assert!(matches!(app.view, View::Reading));
    (app, data, root)
}

fn session(app: &App) -> &Session {
    app.session.as_ref().expect("açık kitap yok")
}

#[test]
fn library_is_sorted_in_turkish_and_opens_on_contents() {
    let (mut app, _, _) = setup(&[("k.epub", epub2_sample()), ("c.epub", epub3_sample())]);
    let titles: Vec<_> = app.library.iter().map(|e| e.display_title()).collect();
    assert_eq!(titles, vec!["Çalıkuşu", "Kuyucaklı Yusuf"]);
    assert_eq!(tap_row(&mut app, &LIBRARY, 0), Some(Refresh::Full));
    assert!(matches!(app.view, View::Contents { page: 0 }));
    tap_row(&mut app, &CONTENTS, 2);
    assert!(matches!(app.view, View::Reading));
    assert_eq!(session(&app).spine, 1);
}

#[test]
fn tap_below_the_last_row_does_nothing() {
    let (mut app, _, _) = setup(&[("c.epub", epub3_sample())]);
    assert_eq!(tap_row(&mut app, &LIBRARY, 5), None);
    tap_row(&mut app, &LIBRARY, 0);
    assert_eq!(tap_row(&mut app, &CONTENTS, 10), None);
}

#[test]
fn page_keys_cross_chapter_boundaries() {
    let (mut app, _, _) = setup(&[("c.epub", epub3_sample())]);
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    key(&mut app, true);
    assert_eq!(session(&app).spine, 1);
    key(&mut app, false);
    assert_eq!(session(&app).spine, 0);
    assert_eq!(key(&mut app, false), None, "kitabın başında geri gidilmez");
}

#[test]
fn reading_position_survives_a_restart() {
    let (mut app, data, root) = reading_long_book();
    for _ in 0..5 {
        key(&mut app, true);
    }
    let before = (session(&app).spine, session(&app).page);
    assert_eq!(before.1, 5);
    app.shutdown();

    let mut app = restart(&data, &root);
    assert_eq!(
        app.progress.values().copied().next().map(|p| p > 0),
        Some(true),
        "kütüphane ilerlemeyi göstermeli"
    );
    tap_row(&mut app, &LIBRARY, 0);
    assert!(
        matches!(app.view, View::Reading),
        "kayıtlı konumdan açılmalı"
    );
    assert_eq!((session(&app).spine, session(&app).page), before);
}

#[test]
fn position_is_saved_every_few_turns_without_shutdown() {
    let (mut app, data, root) = reading_long_book();
    for _ in 0..SAVE_EVERY_TURNS {
        key(&mut app, true);
    }
    // Kapatmadan (ör. pil bitti) yeniden başlat: son kayıt 10. sayfada.
    let mut app2 = restart(&data, &root);
    tap_row(&mut app2, &LIBRARY, 0);
    assert_eq!(session(&app2).page, SAVE_EVERY_TURNS as usize);
    drop(app);
}

#[test]
fn bookmark_is_added_listed_persisted_and_removed() {
    let (mut app, data, root) = reading_long_book();
    for _ in 0..3 {
        key(&mut app, true);
    }
    menu(&mut app, MenuItem::Bookmark);
    assert!(matches!(app.view, View::Reading));
    assert_eq!(session(&app).bookmark_here(), Some(0));
    assert!(!session(&app).state.bookmarks[0].snippet.is_empty());

    // Başa dön, yer imleri listesinden geri gel.
    for _ in 0..3 {
        key(&mut app, false);
    }
    menu(&mut app, MenuItem::Bookmarks);
    assert!(matches!(app.view, View::Bookmarks { page: 0 }));
    tap_row(&mut app, &BOOKMARKS, 0);
    assert_eq!(session(&app).page, 3);

    // Yeniden başlatınca yer imi duruyor; menüden kaldırılabiliyor.
    let mut app = restart(&data, &root);
    tap_row(&mut app, &LIBRARY, 0);
    assert_eq!(session(&app).bookmark_here(), Some(0));
    menu(&mut app, MenuItem::Bookmark);
    assert!(session(&app).state.bookmarks.is_empty());
}

#[test]
fn settings_apply_on_ok_keep_position_and_persist() {
    let (mut app, data, root) = reading_long_book();
    for _ in 0..7 {
        key(&mut app, true);
    }
    let before = session(&app).position();
    let pages_before = session(&app).pages.len();
    menu(&mut app, MenuItem::Settings);
    let size_row = FIELD_PAGES[0]
        .iter()
        .position(|f| *f == Field::Size)
        .unwrap();
    let (_, plus) = app.settings_buttons(size_row);
    tap(&mut app, plus.center());
    tap(&mut app, plus.center());
    assert_eq!(app.settings().font_px, 40.0, "Tamam'dan önce uygulanmamalı");
    let at = app.settings_ok().center();
    tap(&mut app, at);

    assert!(matches!(app.view, View::Reading));
    assert_eq!(app.settings().font_px, 48.0);
    let s = session(&app);
    assert!(s.pages.len() > pages_before);
    assert!(s.pages[s.page].start <= before);
    assert!(
        s.pages
            .get(s.page + 1)
            .is_none_or(|next| before < next.start),
        "konum yeni sayfada değil"
    );

    // Ayar bu kitaba özel: kütüphanede varsayılan, kitapta değişmiş boyut.
    let mut app = restart(&data, &root);
    assert_eq!(app.settings().font_px, 40.0);
    tap_row(&mut app, &LIBRARY, 0);
    assert_eq!(app.settings().font_px, 48.0);
}

#[test]
fn settings_cancel_discards_changes() {
    let (mut app, _, _) = reading_long_book();
    let before = app.settings().clone();
    menu(&mut app, MenuItem::Settings);
    let (_, plus) = app.settings_buttons(0);
    tap(&mut app, plus.center());
    let at = app.settings_cancel().center();
    tap(&mut app, at);
    assert_eq!(app.settings(), &before);
    assert!(matches!(app.view, View::Reading));
}

#[test]
fn night_mode_inverts_the_screen() {
    let (mut app, _, _) = reading_long_book();
    menu(&mut app, MenuItem::Settings);
    press_plus(&mut app, Field::Night);
    let at = app.settings_ok().center();
    tap(&mut app, at);
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    assert_eq!(c.pixels()[0], 0, "gece modunda zemin siyah olmalı");
}

#[test]
fn menu_closes_on_outside_tap_and_returns_to_library() {
    let (mut app, _, _) = reading_long_book();
    middle(&mut app);
    tap(&mut app, (W / 2, 5));
    assert!(
        matches!(app.view, View::Menu),
        "çubuktaki boş yer menüyü kapatmaz"
    );
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2 + 300));
    assert!(
        matches!(app.view, View::Reading),
        "sayfaya dokunmak kapatır"
    );
    menu(&mut app, MenuItem::Library);
    assert!(matches!(app.view, View::Library { .. }));
    assert!(app.session.is_none());
}

#[test]
fn contents_back_button_returns_to_reading() {
    let (mut app, _, _) = reading_long_book();
    key(&mut app, true);
    menu(&mut app, MenuItem::Contents);
    assert!(matches!(app.view, View::Contents { .. }));
    tap(&mut app, super::ui::back_button(W).center());
    assert!(matches!(app.view, View::Reading));
    assert_eq!(session(&app).page, 1);
}

#[test]
fn unreadable_book_shows_turkish_error_and_returns() {
    let (mut app, _, _) = setup(&[("bozuk.epub", b"zip degil".to_vec())]);
    assert!(!app.library[0].readable);
    tap_row(&mut app, &LIBRARY, 0);
    assert!(matches!(
        &app.view,
        View::Message {
            msg: Msg::BookNotEpub,
            ..
        }
    ));
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    tap(&mut app, (10, 10));
    assert!(matches!(app.view, View::Library { .. }));
}

#[test]
fn long_chapter_pages_fit_and_start_in_order() {
    let (app, _, _) = reading_long_book();
    let pages = &session(&app).pages;
    assert!(pages.len() > 5);
    for p in pages {
        assert!(!p.lines.is_empty());
        assert!(p.lines.iter().all(|l| l.baseline <= super::ui::BOTTOM));
    }
    for w in pages.windows(2) {
        assert!(w[0].start < w[1].start);
    }
}

#[test]
fn every_sixth_page_turn_is_a_full_refresh() {
    let (mut app, _, _) = reading_long_book();
    let kinds: Vec<_> = (0..12).map(|_| key(&mut app, true).unwrap()).collect();
    let fulls: Vec<_> = kinds
        .iter()
        .enumerate()
        .filter(|(_, r)| **r == Refresh::Full)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(fulls, vec![5, 11]);
}

#[test]
fn every_screen_draws_without_panicking() {
    let (mut app, _, _) = reading_long_book();
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    for item in [MenuItem::Contents, MenuItem::Bookmarks, MenuItem::Settings] {
        middle(&mut app);
        app.draw(&mut c);
        let index = app.menu_items().iter().position(|m| *m == item).unwrap();
        let at = app.menu_rect(index).center();
        tap(&mut app, at);
        app.draw(&mut c);
        tap(&mut app, super::ui::back_button(W).center());
        if matches!(app.view, View::Settings { .. }) {
            let at = app.settings_cancel().center();
            tap(&mut app, at);
        }
    }
    menu(&mut app, MenuItem::Library);
    app.draw(&mut c);
}

fn pdf_book() -> Vec<u8> {
    crate::pdf::testutil::pdf_with_pages(&["Bir", "Iki", "Uc", "Dort"])
}

fn ink(c: &Canvas) -> usize {
    c.pixels().iter().filter(|&&p| p < 128).count()
}

#[test]
fn pdf_opens_on_its_first_page_and_turns_pages() {
    let (mut app, _, _) = setup(&[("belge.pdf", pdf_book())]);
    assert_eq!(app.library[0].format, crate::library::Format::Pdf);
    tap_row(&mut app, &LIBRARY, 0);
    assert!(matches!(app.view, View::Reading));
    assert_eq!(session(&app).chapter_count(), 4);
    assert!(session(&app).showing_pdf_image());
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    assert!(ink(&c) > 1000, "PDF sayfası çizilmemiş");
    key(&mut app, true);
    key(&mut app, true);
    assert_eq!(session(&app).spine, 2);
    assert_eq!(session(&app).percent(), 75);
    for _ in 0..5 {
        key(&mut app, true);
    }
    assert_eq!(session(&app).spine, 3, "son sayfada durmalı");
}

#[test]
fn pdf_position_and_bookmarks_survive_a_restart() {
    let (mut app, data, root) = setup(&[("belge.pdf", pdf_book())]);
    tap_row(&mut app, &LIBRARY, 0);
    key(&mut app, true);
    key(&mut app, true);
    menu(&mut app, MenuItem::Bookmark);
    assert_eq!(session(&app).state.bookmarks[0].snippet, "Sayfa 3");
    app.shutdown();

    let mut app = restart(&data, &root);
    tap_row(&mut app, &LIBRARY, 0);
    assert_eq!(session(&app).spine, 2);
    assert_eq!(session(&app).bookmark_here(), Some(0));
}

#[test]
fn pdf_contents_lists_pages_and_jumps() {
    let (mut app, _, _) = setup(&[("belge.pdf", pdf_book())]);
    tap_row(&mut app, &LIBRARY, 0);
    menu(&mut app, MenuItem::Contents);
    assert!(matches!(app.view, View::Contents { .. }));
    tap_row(&mut app, &CONTENTS, 3);
    assert!(matches!(app.view, View::Reading));
    assert_eq!(session(&app).spine, 3);
}

#[test]
fn pdf_crop_enlarges_the_content() {
    let (mut app, _, _) = setup(&[("belge.pdf", pdf_book())]);
    tap_row(&mut app, &LIBRARY, 0);
    let mut cropped = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut cropped);

    menu(&mut app, MenuItem::Settings);
    press_plus(&mut app, Field::PdfCrop);
    let at = app.settings_ok().center();
    tap(&mut app, at);
    assert!(!app.settings().pdf_crop);
    let mut whole = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut whole);
    assert!(
        ink(&cropped) > 2 * ink(&whole),
        "kırpınca içerik büyümeli: {} / {}",
        ink(&cropped),
        ink(&whole)
    );
}

/// Ayarlar ekranında bir alanı `times` kez artırıp "Tamam"a dokunur.
fn change_setting(app: &mut App, field: Field, times: usize) {
    menu(app, MenuItem::Settings);
    for _ in 0..times {
        press_plus(app, field);
    }
    let at = app.settings_ok().center();
    tap(app, at);
}

fn screens(app: &App) -> Vec<super::pdf_view::Screen> {
    session(app).pages.iter().filter_map(|p| p.screen).collect()
}

#[test]
fn pdf_width_and_zoom_split_the_page_into_screens() {
    let (mut app, _, _) = setup(&[("metin.pdf", text_pdf())]);
    tap_row(&mut app, &LIBRARY, 0);
    assert_eq!(session(&app).pages.len(), 1, "sayfaya sığdırınca tek ekran");

    change_setting(&mut app, Field::PdfView, 1);
    assert_eq!(app.settings().pdf_view, settings::PdfView::Width);
    change_setting(&mut app, Field::PdfZoom, 3);
    assert_eq!(app.settings().pdf_zoom, 200);
    let s = screens(&app);
    assert_eq!(s.len(), 6, "%200 genişlikte 2 sütun × 3 satır ekran: {s:?}");
    // Okuma sırası: önce soldaki, sonra sağdaki karo; sonra aşağı.
    assert!(s[1].area.x > s[0].area.x && s[1].area.y == s[0].area.y);
    assert!(s[2].area.y > s[0].area.y);
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    assert!(ink(&c) > 1000);

    // Ekranlar bitince sonraki PDF sayfası; geri dönünce önceki sayfanın son ekranı.
    for _ in 0..6 {
        key(&mut app, true);
    }
    assert_eq!((session(&app).spine, session(&app).page), (1, 0));
    key(&mut app, false);
    assert_eq!((session(&app).spine, session(&app).page), (0, 5));

    // Yakınlaştırma değişince okuma sırasındaki yer korunur.
    menu(&mut app, MenuItem::Settings);
    let at = app.settings_page_button().center(); // PDF ayarları
    tap(&mut app, at);
    let row = FIELD_PAGES[1]
        .iter()
        .position(|f| *f == Field::PdfZoom)
        .unwrap();
    let at = app.settings_buttons(row).0.center(); // "−"
    tap(&mut app, at);
    let at = app.settings_ok().center();
    tap(&mut app, at);
    assert_eq!(app.settings().pdf_zoom, 150);
    let s = session(&app);
    assert_eq!(s.spine, 0);
    assert_eq!(s.page, s.pages.len() - 1, "son ekranda kalmalı");
}

#[test]
fn pdf_columns_are_read_one_after_another() {
    let line = |x0: u32, x1: u32, top: u32, bottom: u32| -> String {
        (bottom..top)
            .step_by(14)
            .map(|y| format!("{x0} {y} {} 8 re f ", x1 - x0))
            .collect()
    };
    let page = format!("{}{}", line(72, 290, 720, 100), line(322, 540, 720, 100));
    let (mut app, _, _) = setup(&[(
        "sutun.pdf",
        crate::pdf::testutil::pdf_with_content(&[&page]),
    )]);
    tap_row(&mut app, &LIBRARY, 0);
    change_setting(&mut app, Field::PdfView, 2);
    assert_eq!(app.settings().pdf_view, settings::PdfView::Columns);
    let s = screens(&app);
    assert!(s.len() >= 2, "{s:?}");
    let half = s.len() / 2;
    assert!(
        s[..half].iter().all(|x| x.area.x < 100.0),
        "önce sol sütun: {s:?}"
    );
    assert!(
        s[half..].iter().all(|x| x.area.x > 300.0),
        "sonra sağ sütun: {s:?}"
    );
    // Sütun ekran genişliğine sığar: tüm sayfayı sığdırmaktan büyük ölçek.
    let (pw, _) = session(&app).pdf().unwrap().page_size(0).unwrap();
    assert!(s[0].scale > 1.5 * (W as f32 / pw), "{}", s[0].scale);
}

#[test]
fn raw_touches_become_taps_and_holds_do_not_turn_pages() {
    use crate::event::TouchPhase::{Down, Up};
    let (mut app, _, _) = setup(&[("belge.pdf", pdf_book())]);
    tap_row(&mut app, &LIBRARY, 0);
    let touch = |app: &mut App, phase, x, ms| {
        app.handle(Event::Touch {
            phase,
            x,
            y: LIBRA2_HEIGHT / 2,
            ms,
        })
    };
    // Uzun basma sayfa çevirmez.
    touch(&mut app, Down, W - 50, 0);
    assert_eq!(touch(&mut app, Up, W - 50, 900), None);
    assert_eq!(session(&app).spine, 0);
    // Kısa dokunma sağ üçte birde sonraki sayfa.
    touch(&mut app, Down, W - 50, 1000);
    assert!(touch(&mut app, Up, W - 50, 1100).is_some());
    assert_eq!(session(&app).spine, 1);
}

#[test]
fn annotations_survive_restart_and_font_changes() {
    let (mut app, data, root) = setup(&[("uzun.epub", long_book())]);
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    let s = app.session.as_mut().unwrap();
    let start = Position {
        block: 1,
        offset: 5,
    };
    let end = Position {
        block: 1,
        offset: 20,
    };
    let i = s.add_annotation(start, end, Default::default()).unwrap();
    assert_eq!(s.state.annotations[i].quote, "bir paragraf: a");
    assert_eq!(s.placed, vec![(i, start, end)]);
    assert_eq!(
        s.add_annotation(end, start, Default::default()),
        None,
        "boş aralık"
    );
    app.shutdown();
    app.save_progress();

    let mut app = restart(&data, &root);
    tap_row(&mut app, &LIBRARY, 0);
    assert_eq!(session(&app).state.annotations.len(), 1);
    assert_eq!(session(&app).placed, vec![(0, start, end)]);
    change_setting(&mut app, Field::Size, 3);
    assert_eq!(
        session(&app).placed,
        vec![(0, start, end)],
        "yazı ayarı vurguyu kaydırmamalı"
    );
    app.session.as_mut().unwrap().remove_annotation(0);
    assert!(session(&app).placed.is_empty());
}

/// Okunan sayfanın `line`. satırındaki `n`. glifin ekran noktası.
fn glyph_point(app: &App, line: usize, n: usize) -> (u32, u32) {
    let s = session(app);
    let l = &s.pages[s.page].lines[line];
    let (g, gx) = &l.line.glyphs[n];
    (
        (app.settings().margin as f32 + gx + g.advance / 2.0) as u32,
        (l.top + l.height / 2) as u32,
    )
}

#[test]
fn hold_drag_and_highlight_then_remove_it() {
    let (mut app, _, _) = setup(&[("uzun.epub", long_book())]);
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    // 0. satır başlık ("Başlık"); 1. satır paragrafın başı: "Uzun bir paragraf: ..."
    let (x, y) = glyph_point(&app, 1, 6); // "bir" içinde
    app.handle(Event::Hold { x, y });
    let sel = app.selection.unwrap();
    let text = &session(&app).blocks[1].text;
    assert_eq!(&text[sel.start.offset..sel.end.offset], "bir");
    let (x2, y2) = glyph_point(&app, 1, 12); // "paragraf" içinde
    app.handle(Event::HoldMove { x: x2, y: y2 });
    app.handle(Event::HoldEnd { x: x2, y: y2 });
    assert!(matches!(app.view, View::Selected));
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);

    let at = app.action_rect(0).center(); // Vurgula
    tap(&mut app, at);
    assert!(matches!(app.view, View::Reading));
    assert!(app.selection.is_none());
    let s = session(&app);
    assert_eq!(s.state.annotations[0].quote, "bir paragraf");
    assert_eq!(s.placed.len(), 1);
    // Vurgu zemini çizilir: satırda açık gri piksel var.
    app.draw(&mut c);
    let row = |y: u32| &c.pixels()[(y * W) as usize..((y + 1) * W) as usize];
    assert!(row(y).contains(&select::HIGHLIGHT_GRAY));

    // Vurguya dokunmak sayfa çevirmez, işlem çubuğunu açar.
    tap(&mut app, (x, y));
    assert!(matches!(app.view, View::Annotation { index: 0 }));
    assert_eq!(session(&app).page, 0);
    let at = app.action_rect(3).center(); // Vurguyu sil
    tap(&mut app, at);
    assert!(session(&app).state.annotations.is_empty());
    assert!(matches!(app.view, View::Reading));
}

#[test]
fn highlight_kind_and_tag_are_changed_remembered_and_searched() {
    let (mut app, data, root) = setup(&[("uzun.epub", long_book())]);
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    let (x, y) = glyph_point(&app, 1, 6);
    app.handle(Event::Hold { x, y });
    app.handle(Event::HoldEnd { x, y });
    let at = app.action_rect(0).center();
    tap(&mut app, at); // Vurgula
    tap(&mut app, (x, y));
    assert!(matches!(app.view, View::Annotation { index: 0 }));

    // Tür: gri → çizgi; çubuk açık kalır, çizgi siyah çizilir, gri zemin yok.
    let at = app.action_rect(1).center();
    tap(&mut app, at);
    assert!(matches!(app.view, View::Annotation { index: 0 }));
    assert_eq!(
        session(&app).state.annotations[0].mark,
        annotations::Mark::Underline
    );
    assert_eq!(app.settings().highlight, annotations::Mark::Underline);
    tap(&mut app, (10, 10));
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    let row = |c: &Canvas, y: u32| c.pixels()[(y * W) as usize..((y + 1) * W) as usize].to_vec();
    assert!(!row(&c, y).contains(&select::HIGHLIGHT_GRAY));

    // Etiket yazılır.
    tap(&mut app, (x, y));
    let at = app.action_rect(2).center();
    tap(&mut app, at);
    type_text(&mut app, "önemli");
    form_button(&mut app, form::Button::Submit);
    assert!(matches!(app.view, View::Reading));
    assert_eq!(
        session(&app).state.annotations[0].tag.as_deref(),
        Some("önemli")
    );

    // Yeni vurgu son seçilen türle.
    let (x2, y2) = glyph_point(&app, 1, 12);
    app.handle(Event::Hold { x: x2, y: y2 });
    app.handle(Event::HoldEnd { x: x2, y: y2 });
    let at = app.action_rect(0).center();
    tap(&mut app, at);
    assert_eq!(
        session(&app).state.annotations[1].mark,
        annotations::Mark::Underline
    );

    // Etiketle arama; kayıtlar kalıcı.
    menu(&mut app, MenuItem::Annotations);
    app.annotation_query = Some("ÖNEMLİ".into());
    assert_eq!(app.annotation_rows(), [0]);
    app.draw(&mut c);
    app.shutdown();
    let mut app = restart(&data, &root);
    tap_row(&mut app, &LIBRARY, 0);
    let a = &session(&app).state.annotations[0];
    assert_eq!(
        (a.mark, a.tag.as_deref()),
        (annotations::Mark::Underline, Some("önemli"))
    );
}

#[test]
fn a_tap_elsewhere_cancels_the_selection() {
    let (mut app, _, _) = setup(&[("uzun.epub", long_book())]);
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    let (x, y) = glyph_point(&app, 2, 3);
    app.handle(Event::Hold { x, y });
    app.handle(Event::HoldEnd { x, y });
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2));
    assert!(matches!(app.view, View::Reading));
    assert!(app.selection.is_none());
    assert!(session(&app).state.annotations.is_empty());
    // Satır arası boşluğa ya da kenara uzun basmak seçim başlatmaz.
    app.handle(Event::Hold { x: 5, y: 5 });
    assert!(app.selection.is_none());
}

#[test]
fn pdf_image_highlights_are_shared_with_reflow() {
    let (mut app, data, root) = setup(&[("metin.pdf", text_pdf())]);
    tap_row(&mut app, &LIBRARY, 0);
    assert!(session(&app).showing_pdf_image());
    assert!(
        session(&app).pdf_text.is_none(),
        "metin yalnızca gerektiğinde çıkarılır"
    );
    // "Birinci" başlığı: sayfada (72, 740) taban çizgisi, 20 punto.
    let screen = session(&app).pages[0].screen.unwrap();
    let at = |x: f32, y: f32| {
        let r = super::pdf_view::to_screen(
            &screen,
            (W, LIBRA2_HEIGHT),
            crate::pdf::Area {
                x,
                y,
                w: 0.0,
                h: 0.0,
            },
        );
        (r.x as u32, r.y as u32)
    };
    let (x, y) = at(90.0, 792.0 - 745.0);
    app.handle(Event::Hold { x, y });
    app.handle(Event::HoldEnd { x, y });
    assert!(matches!(app.view, View::Selected));
    let ok = app.action_rect(0).center();
    tap(&mut app, ok);
    assert_eq!(session(&app).state.annotations[0].quote, "Birinci");
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    let row = &c.pixels()[(y * W) as usize..((y + 1) * W) as usize];
    assert!(
        row.contains(&select::HIGHLIGHT_GRAY),
        "vurgu görüntü üzerine çizilmeli"
    );
    app.shutdown();

    let mut app = restart(&data, &root);
    tap_row(&mut app, &LIBRARY, 0);
    assert_eq!(session(&app).placed.len(), 1, "görüntüde vurgu yerleşmeli");
    toggle_reflow(&mut app);
    let s = session(&app);
    let (_, a, b) = s.placed[0];
    assert_eq!(&s.blocks[a.block].text[a.offset..b.offset], "Birinci");
}

/// Not ekranında klavye tuşuna dokunur.
fn press_key(app: &mut App, key: keyboard::Key) {
    let View::Note { keyboard, .. } = &app.view else {
        panic!("not ekranı açık değil");
    };
    let r = keyboard
        .keys(W)
        .into_iter()
        .find(|(k, _)| *k == key)
        .map(|(_, r)| r)
        .expect("tuş klavyede olmalı");
    tap(app, r.center());
}

#[test]
fn notes_are_typed_saved_shown_and_cleared() {
    use keyboard::Key::*;
    let (mut app, data, root) = setup(&[("uzun.epub", long_book())]);
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    let (x, y) = glyph_point(&app, 1, 6);
    app.handle(Event::Hold { x, y });
    app.handle(Event::HoldEnd { x, y });
    let at = app.action_rect(1).center(); // Not ekle
    tap(&mut app, at);
    assert!(matches!(app.view, View::Note { .. }));
    assert_eq!(
        session(&app).state.annotations.len(),
        1,
        "not eklemek vurgular"
    );
    for k in [
        Shift,
        Char('i'),
        Char('ş'),
        Space,
        Char('ı'),
        Char('k'),
        Backspace,
    ] {
        press_key(&mut app, k);
    }
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    let at = app.note_buttons().1.center(); // Kaydet
    tap(&mut app, at);
    assert!(matches!(app.view, View::Reading));
    assert_eq!(
        session(&app).state.annotations[0].note.as_deref(),
        Some("İş ı")
    );
    app.shutdown();

    let mut app = restart(&data, &root);
    tap_row(&mut app, &LIBRARY, 0);
    tap(&mut app, (x, y));
    assert!(matches!(app.view, View::Annotation { index: 0 }));
    app.draw(&mut c);
    let at = app.action_rect(0).center(); // Notu düzenle
    tap(&mut app, at);
    let View::Note { draft, .. } = &app.view else {
        panic!("not ekranı açılmalı");
    };
    assert_eq!(draft, "İş ı");
    for _ in 0..4 {
        press_key(&mut app, Backspace);
    }
    let at = app.note_buttons().1.center();
    tap(&mut app, at);
    let a = &session(&app).state.annotations[0];
    assert_eq!(a.note, None, "boş not kaldırılır, vurgu kalır");
}

#[test]
fn annotation_list_jumps_and_notes_export_to_markdown() {
    let (mut app, _, root) = setup(&[("uzun.epub", long_book())]);
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    for _ in 0..3 {
        key(&mut app, true);
    }
    let s = app.session.as_mut().unwrap();
    let at = s.pages[s.page].start;
    let end = Position {
        offset: at.offset + 10,
        ..at
    };
    let i = s.add_annotation(at, end, Default::default()).unwrap();
    s.set_note(i, Some("Önemli yer.".into()));
    let quote = s.state.annotations[i].quote.clone();
    key(&mut app, false);
    key(&mut app, false);

    menu(&mut app, MenuItem::Annotations);
    assert!(matches!(app.view, View::Annotations { page: 0 }));
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    tap_row(&mut app, &super::screens::ANNOTATIONS, 0);
    assert!(matches!(app.view, View::Reading));
    assert_eq!(session(&app).page, 3, "vurgunun sayfasına gitmeli");

    menu(&mut app, MenuItem::ExportNotes);
    assert!(matches!(
        &app.view,
        View::Message {
            msg: Msg::NotesExported { count: 1, .. },
            ..
        }
    ));
    let md = std::fs::read_to_string(root.join("uzun - notlar.md")).unwrap();
    assert!(md.contains(&format!("> {quote}")), "{md}");
    assert!(md.contains("Not: Önemli yer."), "{md}");
}

#[test]
fn pdf_notes_export_as_standard_annotations() {
    let (mut app, _, root) = setup(&[("metin.pdf", text_pdf())]);
    tap_row(&mut app, &LIBRARY, 0);
    let s = app.session.as_mut().unwrap();
    s.load_page_text();
    let start = Position {
        block: 0,
        offset: 0,
    };
    let end = Position {
        block: 1,
        offset: 6,
    }; // başlık + paragrafın ilk kelimesi, iki satır
    let i = s.add_annotation(start, end, Default::default()).unwrap();
    s.set_note(i, Some("Başlıkla birlikte.".into()));
    let original = std::fs::read(root.join("metin.pdf")).unwrap();

    menu(&mut app, MenuItem::ExportNotes);
    assert!(root.join("metin - notlar.md").exists());
    let out = lopdf::Document::load(root.join("metin - notlu.pdf")).unwrap();
    let page = out.get_pages()[&1];
    let annots = out
        .get_dictionary(page)
        .unwrap()
        .get(b"Annots")
        .unwrap()
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(annots.len(), 2, "her satır için bir vurgu açıklaması");
    let notes: Vec<_> = annots
        .iter()
        .map(|r| out.get_dictionary(r.as_reference().unwrap()).unwrap())
        .filter(|d| d.get(b"Contents").is_ok())
        .collect();
    assert_eq!(notes.len(), 1, "not bir kez yazılır");
    assert_eq!(
        std::fs::read(root.join("metin.pdf")).unwrap(),
        original,
        "orijinal değişmemeli"
    );
}

#[test]
fn backup_restores_notes_and_settings_on_a_fresh_install() {
    let (mut app, data, root) = setup(&[("uzun.epub", long_book())]);
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    let s = app.session.as_mut().unwrap();
    let i = s
        .add_annotation(
            Position {
                block: 1,
                offset: 0,
            },
            Position {
                block: 1,
                offset: 4,
            },
            Default::default(),
        )
        .unwrap();
    s.set_note(i, Some("yedeklenecek".into()));
    change_setting(&mut app, Field::Size, 2);
    let size = app.settings().font_px;
    menu(&mut app, MenuItem::Library);

    tap(&mut app, super::ui::back_button(W).center()); // Yedek
    assert!(matches!(app.view, View::Backup));
    let at = app.backup_buttons().0.center();
    tap(&mut app, at);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::BackupSaved { books: 1 },
            ..
        }
    ));
    assert!(root.join(super::backup::BACKUP_FILE).exists());

    // Yeni kurulum: veri klasörü boş, kütüphane aynı.
    std::fs::remove_dir_all(data.join("books")).unwrap();
    std::fs::remove_file(data.join("settings.json")).unwrap();
    let mut app = restart(&data, &root);
    assert_eq!(app.settings().font_px, Settings::default().font_px);
    tap(&mut app, super::ui::back_button(W).center());
    let at = app.backup_buttons().1.center();
    tap(&mut app, at);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::RestoreDone { books: 1 },
            ..
        }
    ));
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2)); // mesajdan kütüphaneye
    tap_row(&mut app, &LIBRARY, 0);
    assert_eq!(
        app.settings().font_px,
        size,
        "kitabın yazı ayarı yedekten geldi"
    );
    let a = &session(&app).state.annotations;
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].note.as_deref(), Some("yedeklenecek"));
    assert_eq!(a[0].quote, "Uzun");
}

#[test]
fn restoring_without_a_backup_says_so() {
    let (mut app, _, _) = setup(&[("uzun.epub", long_book())]);
    tap(&mut app, super::ui::back_button(W).center());
    let at = app.backup_buttons().1.center();
    tap(&mut app, at);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::NoBackup,
            ..
        }
    ));
}

/// Okunan sayfada `word` kelimesinin ilk harfinin ekran noktası.
fn word_point(app: &App, word: &str) -> (u32, u32) {
    let s = session(app);
    for (li, l) in s.pages[s.page].lines.iter().enumerate() {
        let text = &s.blocks[l.block].text;
        if let Some(n) = l
            .line
            .glyphs
            .iter()
            .position(|(g, _)| text[g.cluster..].starts_with(word))
        {
            return glyph_point(app, li, n);
        }
    }
    panic!("{word} sayfada yok");
}

fn select_and(app: &mut App, word: &str, action: usize) {
    let (x, y) = word_point(app, word);
    app.handle(Event::Hold { x, y });
    app.handle(Event::HoldEnd { x, y });
    let at = app.action_rect(action).center();
    tap(app, at);
}

#[test]
fn a_selected_word_is_looked_up_by_its_root() {
    let (mut app, _, root) = setup(&[("uzun.epub", long_book())]);
    let dir = root.join("Sözlükler");
    std::fs::create_dir_all(&dir).unwrap();
    crate::dict::testutil::stardict(
        &dir,
        "deneme",
        &[("ağaç", "<b>isim</b> Gövdeli bitki.")],
        &[],
        true,
    );
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    select_and(&mut app, "ağaçlar", 2); // Sözlük
    assert!(matches!(app.view, View::Lookup { page: 0 }));
    assert!(!app.lookup.as_ref().unwrap().pages[0].lines.is_empty());
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    assert!(ink(&c) > 1000, "tanım çizilmeli");
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2));
    assert!(matches!(app.view, View::Reading));
    assert!(app.lookup.is_none());

    select_and(&mut app, "paragraf", 2);
    assert!(matches!(
        &app.view,
        View::Message { msg: Msg::NotInDictionary(w), .. } if w == "paragraf"
    ));
}

#[test]
fn looked_up_words_are_saved_listed_and_backed_up() {
    let (mut app, data, root) = setup(&[("uzun.epub", long_book())]);
    let dir = root.join("Sözlükler");
    std::fs::create_dir_all(&dir).unwrap();
    crate::dict::testutil::stardict(&dir, "deneme", &[("ağaç", "Gövdeli bitki.")], &[], false);
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    select_and(&mut app, "ağaçlar", 2);
    assert!(matches!(app.view, View::Lookup { .. }));
    let at = app.word_button().center();
    tap(&mut app, at); // Listeye ekle
    assert_eq!(app.words.len(), 1);
    assert_eq!(
        (app.words[0].word.as_str(), app.words[0].seen.as_str()),
        ("ağaç", "ağaçlar")
    );
    assert!(matches!(app.view, View::Lookup { .. }), "tanım açık kalır");
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2));

    menu(&mut app, MenuItem::Words);
    assert!(matches!(app.view, View::Words { page: 0 }));
    app.draw(&mut c);
    tap_row(&mut app, &super::screens::WORDS, 0);
    assert!(
        matches!(app.view, View::Lookup { .. }),
        "listeden yeniden bakılır"
    );

    // Yedek ve yeni kurulum.
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2));
    menu(&mut app, MenuItem::Library);
    tap(&mut app, super::ui::back_button(W).center());
    let at = app.backup_buttons().0.center();
    tap(&mut app, at);
    std::fs::remove_file(data.join("words.json")).unwrap();
    let mut app = restart(&data, &root);
    assert!(app.words.is_empty());
    tap(&mut app, super::ui::back_button(W).center());
    let at = app.backup_buttons().1.center();
    tap(&mut app, at);
    assert_eq!(app.words.len(), 1);

    // Çıkarma kalıcı.
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2));
    tap_row(&mut app, &LIBRARY, 0);
    let saved = app.words[0].clone();
    app.toggle_saved(&saved.word, &saved.seen);
    assert!(restart(&data, &root).words.is_empty());
}

#[test]
fn looking_up_without_dictionaries_explains_where_to_put_them() {
    let (mut app, _, _) = setup(&[("uzun.epub", long_book())]);
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    select_and(&mut app, "ağaçlar", 2);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::NoDictionaries,
            ..
        }
    ));
}

#[test]
fn broken_pdf_shows_turkish_error() {
    let (mut app, _, _) = setup(&[("bozuk.pdf", b"%PDF-1.4 bozuk".to_vec())]);
    tap_row(&mut app, &LIBRARY, 0);
    assert!(matches!(
        &app.view,
        View::Message {
            msg: Msg::PdfDamaged,
            ..
        }
    ));
}

/// Her sayfada bir başlık ve uzun bir paragraf: yeniden akıtınca birden çok ekran tutar.
fn text_pdf() -> Vec<u8> {
    let long = "Bu uzun paragraf yeniden akitma icin birden cok satir ve ekran tutacak kadar metin icerir.";
    let page = |title: &str| {
        let mut c = format!("BT /F1 20 Tf 72 740 Td ({title}) Tj ET BT /F1 11 Tf 72 710 Td");
        for _ in 0..45 {
            c.push_str(&format!(" ({long}) Tj 0 -13 Td"));
        }
        c.push_str(" ET");
        c
    };
    let pages = [page("Birinci"), page("Ikinci")];
    crate::pdf::testutil::pdf_with_content(&pages.iter().map(String::as_str).collect::<Vec<_>>())
}

fn toggle_reflow(app: &mut App) {
    menu(app, MenuItem::Settings);
    press_plus(app, Field::PdfReflow);
    let at = app.settings_ok().center();
    tap(app, at);
}

#[test]
fn pdf_reflow_shows_text_screens_and_keeps_the_page() {
    let (mut app, _, _) = setup(&[("metin.pdf", text_pdf())]);
    tap_row(&mut app, &LIBRARY, 0);
    key(&mut app, true);
    assert_eq!(session(&app).spine, 1);

    toggle_reflow(&mut app);
    assert!(app.settings().pdf_reflow);
    let s = session(&app);
    assert_eq!(s.spine, 1, "yeniden akıtınca aynı PDF sayfasında kalmalı");
    assert!(!s.showing_pdf_image());
    assert!(s.pages.len() > 1, "uzun sayfa birden çok ekran tutmalı");
    assert!(s.blocks.iter().any(|b| b.text == "Ikinci"));

    // Ekranlar arasında ilerleyip sonraki PDF sayfasına geçilmez, son ekrandan sonra geçilir.
    let screens = s.pages.len();
    for _ in 0..screens {
        key(&mut app, true);
    }
    assert_eq!(session(&app).spine, 1, "son sayfada durmalı");
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);

    toggle_reflow(&mut app);
    assert!(session(&app).showing_pdf_image());
    assert_eq!(session(&app).spine, 1);
}

#[test]
fn reflow_falls_back_to_the_image_on_pages_without_text() {
    let (mut app, _, _) = setup(&[(
        "belge.pdf",
        crate::pdf::testutil::pdf_with_content(&["0 0 1 rg 72 600 200 50 re f"]),
    )]);
    tap_row(&mut app, &LIBRARY, 0);
    toggle_reflow(&mut app);
    assert!(
        session(&app).showing_pdf_image(),
        "metinsiz sayfa görüntü olarak kalmalı"
    );
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    assert!(ink(&c) > 1000);
}

/// Taranmış sayfa taklidi: "Merhaba Dunya" yazısının görüntüsü, metin katmanı yok.
fn scanned_pdf() -> Vec<u8> {
    use crate::pdf::{Area, PdfDoc};
    let text = PdfDoc::from_bytes(crate::pdf::testutil::minimal_pdf("Merhaba Dunya")).unwrap();
    let area = Area {
        x: 60.0,
        y: 50.0,
        w: 340.0,
        h: 50.0,
    };
    let image = text.render_area(0, area, 4.0).unwrap();
    crate::pdf::testutil::pdf_with_image(&image, area.w, area.h)
}

#[test]
fn ocr_reads_a_scanned_page_and_caches_the_result() {
    if std::env::var_os("LEGENTIA_TESSERACT").is_none() {
        return; // Tesseract yok (nix develop dışı); OCR testi atlanır.
    }
    let (mut app, data, root) = setup(&[("tarama.pdf", scanned_pdf())]);
    tap_row(&mut app, &LIBRARY, 0);
    toggle_reflow(&mut app);
    assert!(
        session(&app).showing_pdf_image(),
        "OCR kapalıyken görüntü kalmalı"
    );

    menu(&mut app, MenuItem::Settings);
    press_plus(&mut app, Field::Ocr);
    let at = app.settings_ok().center();
    tap(&mut app, at);
    let text: String = session(&app)
        .blocks
        .iter()
        .map(|b| b.text.clone())
        .collect();
    assert!(text.contains("Merhaba"), "OCR metni okunmadı: {text:?}");
    assert!(!session(&app).showing_pdf_image());

    // Önbellek: kitap kimliği klasöründe 0.json.
    let id = session(&app).id.clone();
    assert!(data.join("ocr").join(&id).join("0.json").exists());
    drop(app);
    let mut app = restart(&data, &root);
    tap_row(&mut app, &LIBRARY, 0);
    let text: String = session(&app)
        .blocks
        .iter()
        .map(|b| b.text.clone())
        .collect();
    assert!(text.contains("Merhaba"));
}

#[test]
fn settings_rows_never_overlap_the_bottom_buttons() {
    // Hata: ayar sayısı artınca son satırın "+" düğmesi "Tamam"ın üstüne taşmıştı.
    let (app, _, _) = setup(&[]);
    for fields in FIELD_PAGES {
        let (_, plus) = app.settings_buttons(fields.len() - 1);
        assert!(
            plus.y + plus.h <= app.settings_page_button().y,
            "{} satır sığmıyor",
            fields.len()
        );
    }
}

#[test]
fn saves_a_searchable_copy_without_touching_the_original() {
    if std::env::var_os("LEGENTIA_TESSERACT").is_none() {
        return; // Tesseract yok; OCR testi atlanır.
    }
    let (mut app, _, root) = setup(&[("tarama.pdf", scanned_pdf())]);
    let original = std::fs::read(root.join("tarama.pdf")).unwrap();
    tap_row(&mut app, &LIBRARY, 0);
    menu(&mut app, MenuItem::SaveSearchable);
    assert!(
        matches!(
            &app.view,
            View::Message {
                msg: Msg::SearchableSaved { pages: 1, .. },
                ..
            }
        ),
        "beklenmeyen ekran"
    );
    assert_eq!(
        std::fs::read(root.join("tarama.pdf")).unwrap(),
        original,
        "orijinal değişmemeli"
    );

    let copy = crate::pdf::PdfDoc::open(&root.join("tarama - aranabilir.pdf")).unwrap();
    let text: String = copy
        .page_blocks(0)
        .unwrap()
        .iter()
        .map(|b| b.text.clone())
        .collect();
    assert!(text.contains("Merhaba Dunya"), "katman metni: {text:?}");

    // Mesaja dokununca okumaya dönülür.
    tap(&mut app, (10, 10));
    assert!(matches!(app.view, View::Reading));
}

#[test]
fn epub_menu_has_no_searchable_pdf_item() {
    let (mut app, _, _) = setup(&[("c.epub", epub3_sample())]);
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    assert!(!app.menu_items().contains(&MenuItem::SaveSearchable));
}

// --- Kataloglar ---

const ROOT_FEED: &str = r#"<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Deneme Katalog</title>
  <link rel="search" type="application/atom+xml" href="/ara?q={searchTerms}"/>
  <link rel="next" type="application/atom+xml" href="/sayfa2"/>
  <entry><title>Yeni kitaplar</title><content>Son eklenenler</content>
    <link type="application/atom+xml;profile=opds-catalog" href="/yeni"/></entry>
  <entry><title>Çalıkuşu</title><author><name>Reşat Nuri</name></author>
    <summary>Feride'nin hikâyesi.</summary>
    <link rel="http://opds-spec.org/acquisition" type="application/epub+zip" href="/kitap.epub"/></entry>
  <entry><title>Satılık</title>
    <link rel="http://opds-spec.org/acquisition/buy" type="application/epub+zip" href="/al"/></entry>
</feed>"#;

fn one_entry_feed(title: &str) -> Vec<u8> {
    format!(
        r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>{title}</title><entry><title>{title}</title>
        <link rel="http://opds-spec.org/acquisition" type="application/epub+zip" href="/k.epub"/></entry></feed>"#
    )
    .into_bytes()
}

/// Taklit OPDS sunucusu; /gizli "a:b" ister, /yavas geç yanıt verir.
fn opds_server() -> String {
    crate::net::testserver::serve(|path, headers| match path {
        "/opds" => (200, ROOT_FEED.as_bytes().to_vec()),
        "/yeni" => (200, one_entry_feed("Yeni")),
        "/sayfa2" => (200, one_entry_feed("İkinci sayfa")),
        p if p.starts_with("/ara?q=") => (200, one_entry_feed(&format!("Sonuç {}", &p[7..]))),
        "/gizli" if headers.contains("Basic YTpi") => (200, one_entry_feed("Gizli")),
        "/gizli" => (401, Vec::new()),
        "/yavas" => {
            std::thread::sleep(std::time::Duration::from_secs(2));
            (200, one_entry_feed("Geç"))
        }
        _ => (200, b"<html><body>katalog degil</body></html>".to_vec()),
    })
}

fn local_catalog(url: String) -> catalog::Catalog {
    catalog::Catalog {
        name: "Yerel".into(),
        url,
        auth: None,
        members_only: false,
    }
}

/// Arka plan isteği bitene kadar `Tick` gönderir.
fn settle(app: &mut App) {
    for _ in 0..2000 {
        if !app.wants_ticks() {
            return;
        }
        app.handle(Event::Tick);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("istek bitmedi");
}

fn open_catalogs(app: &mut App) {
    tap(
        app,
        super::ui::header_button(W, super::screens::CATALOGS_BUTTON_W).center(),
    );
}

fn feed_titles(app: &App) -> Vec<String> {
    let f = app.browser.feeds.last().expect("açık katalog sayfası");
    f.feed.entries.iter().map(|e| e.title.clone()).collect()
}

fn form(app: &App) -> &form::Form {
    let View::Form(f) = &app.view else {
        panic!("form açık değil");
    };
    f
}

/// Formun odaktaki alanına ekran klavyesiyle yazar (gerekirse işaret katmanına geçer).
fn type_text(app: &mut App, text: &str) {
    use keyboard::Key;
    for c in text.chars() {
        let key = if c == ' ' { Key::Space } else { Key::Char(c) };
        for _ in 0..2 {
            let kb = form(app).keyboard.clone();
            let keys = kb.keys(W);
            if let Some((_, r)) = keys.iter().find(|(k, _)| *k == key) {
                tap(app, r.center());
                break;
            }
            let (_, layer) = keys.iter().find(|(k, _)| *k == Key::Layer).unwrap();
            tap(app, layer.center());
        }
    }
}

fn form_button(app: &mut App, b: form::Button) {
    let buttons = form(app).buttons();
    let i = buttons.iter().position(|x| *x == b).unwrap();
    tap(app, form::button_rect(W, buttons.len(), i).center());
}

#[test]
fn catalog_is_browsed_paged_searched_and_left() {
    let url = opds_server();
    let (mut app, _, _) = setup(&[]);
    app.browser.catalogs = vec![local_catalog(format!("{url}/opds"))];
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    open_catalogs(&mut app);
    assert!(matches!(app.view, View::Catalogs { .. }));
    app.draw(&mut c);

    tap_row(&mut app, &catalog::CATALOGS, 0);
    assert!(matches!(app.view, View::Loading) && app.wants_ticks());
    app.draw(&mut c);
    settle(&mut app);
    assert!(matches!(app.view, View::Feed));
    assert_eq!(feed_titles(&app), ["Yeni kitaplar", "Çalıkuşu", "Satılık"]);
    app.draw(&mut c);

    // Gezinme girdisi yeni bir kat açar; "Geri" önceki kata döner.
    tap_row(&mut app, &catalog::FEED, 0);
    settle(&mut app);
    assert_eq!(feed_titles(&app), ["Yeni"]);
    tap(&mut app, super::ui::back_button(W).center());
    assert_eq!(feed_titles(&app).len(), 3);

    // Kitap ayrıntısı.
    tap_row(&mut app, &catalog::FEED, 1);
    assert!(matches!(app.view, View::Entry { index: 1 }));
    app.draw(&mut c);
    tap(&mut app, super::ui::back_button(W).center());
    assert!(matches!(app.view, View::Feed));

    // Yalnızca satılık olan girdi açıklanır.
    tap_row(&mut app, &catalog::FEED, 2);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::EntryUnavailable,
            ..
        }
    ));
    tap(&mut app, (10, 10));
    assert!(matches!(app.view, View::Feed));

    // Son satır "Sonraki sayfa".
    tap_row(&mut app, &catalog::FEED, 3);
    settle(&mut app);
    assert_eq!(feed_titles(&app), ["İkinci sayfa"]);
    tap(&mut app, super::ui::back_button(W).center());

    // Arama: boş sorgu reddedilir, Türkçe harfler kodlanır.
    tap(&mut app, super::ui::header_button(W, 170).center());
    app.draw(&mut c);
    form_button(&mut app, form::Button::Submit);
    assert_eq!(form(&app).error, Some(Msg::QueryRequired));
    type_text(&mut app, "çay");
    form_button(&mut app, form::Button::Submit);
    settle(&mut app);
    assert_eq!(feed_titles(&app), ["Sonuç %C3%A7ay"]);

    for _ in 0..2 {
        tap(&mut app, super::ui::back_button(W).center());
    }
    assert!(matches!(app.view, View::Catalogs { .. }));
    tap(&mut app, super::ui::back_button(W).center());
    assert!(matches!(app.view, View::Library { .. }));
}

#[test]
fn catalogs_are_added_validated_edited_and_deleted() {
    let (mut app, data, root) = setup(&[]);
    assert_eq!(
        app.browser.catalogs.len(),
        2,
        "Gutenberg ve Standard Ebooks"
    );
    open_catalogs(&mut app);
    tap(&mut app, super::ui::header_button(W, 170).center());
    assert_eq!(form(&app).fields[1].value, "");
    form_button(&mut app, form::Button::Submit);
    assert_eq!(form(&app).error, Some(Msg::NameRequired));
    type_text(&mut app, "ev");
    form_button(&mut app, form::Button::Submit);
    assert_eq!(form(&app).error, Some(Msg::InvalidUrl));
    tap(&mut app, form::field_rects(W, 1).0.center());
    type_text(&mut app, "kitap.ev:8080/opds");
    tap(&mut app, form::field_rects(W, 2).0.center());
    type_text(&mut app, "ali");
    tap(&mut app, form::field_rects(W, 3).0.center());
    type_text(&mut app, "gizli");
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    form_button(&mut app, form::Button::Submit);
    assert!(matches!(app.view, View::Catalogs { .. }));

    let mut app = restart(&data, &root);
    let added = app.browser.catalogs.last().unwrap().clone();
    assert_eq!(added.name, "ev");
    assert_eq!(
        added.url, "http://kitap.ev:8080/opds",
        "kısaltma tamamlandı"
    );
    assert_eq!(
        added.auth,
        Some(crate::net::Auth {
            user: "ali".into(),
            password: "gizli".into()
        })
    );

    // Uzun basma düzenleme formunu açar; değişiklik ve silme kalıcı.
    open_catalogs(&mut app);
    let (x, y) = catalog::CATALOGS.row_rect(W, 0).center();
    app.handle(Event::Hold { x, y });
    assert!(matches!(form(&app).kind, form::FormKind::EditCatalog(0)));
    assert!(form(&app).fields[3].secret);
    type_text(&mut app, " 2");
    form_button(&mut app, form::Button::Submit);
    assert_eq!(app.browser.catalogs[0].name, "Project Gutenberg 2");
    let (x, y) = catalog::CATALOGS.row_rect(W, 1).center();
    app.handle(Event::Hold { x, y });
    form_button(&mut app, form::Button::Delete);
    let app = restart(&data, &root);
    let names: Vec<_> = app
        .browser
        .catalogs
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["Project Gutenberg 2", "ev"]);
}

#[test]
fn failed_or_cancelled_requests_return_safely() {
    let url = opds_server();
    let (mut app, _, _) = setup(&[]);
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    let mut load = |app: &mut App, catalog: catalog::Catalog| {
        app.browser.catalogs = vec![catalog];
        app.view = View::Library { page: 0 };
        open_catalogs(app);
        tap_row(app, &catalog::CATALOGS, 0);
        settle(app);
        app.draw(&mut c);
    };
    let expect_message = |app: &mut App, expected: Msg| {
        let View::Message { msg, .. } = &app.view else {
            panic!("hata mesajı bekleniyordu");
        };
        assert_eq!(*msg, expected);
        tap(app, (10, 10));
        assert!(matches!(app.view, View::Catalogs { .. }));
    };

    load(&mut app, local_catalog(format!("{url}/gizli")));
    expect_message(&mut app, Msg::NetUnauthorized);
    let mut with_auth = local_catalog(format!("{url}/gizli"));
    with_auth.auth = Some(crate::net::Auth {
        user: "a".into(),
        password: "b".into(),
    });
    load(&mut app, with_auth);
    assert_eq!(feed_titles(&app), ["Gizli"]);

    load(&mut app, local_catalog(format!("{url}/html")));
    expect_message(&mut app, Msg::NotOpds);
    load(&mut app, local_catalog("http://127.0.0.1:1/opds".into()));
    expect_message(&mut app, Msg::NetOffline);

    // Beklerken vazgeçilir.
    app.browser.catalogs = vec![local_catalog(format!("{url}/yavas"))];
    tap_row(&mut app, &catalog::CATALOGS, 0);
    assert!(matches!(app.view, View::Loading));
    tap(&mut app, (10, 10));
    assert!(matches!(app.view, View::Loading), "yalnızca Vazgeç bırakır");
    let cancel = app.loading_cancel().center();
    tap(&mut app, cancel);
    assert!(matches!(app.view, View::Catalogs { .. }));
    // Geç gelen yanıt ekranı değiştirmez.
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!app.wants_ticks());
    assert_eq!(app.handle(Event::Tick), None);
}

#[test]
fn catalogs_are_closed_when_the_network_is_off() {
    let (mut app, _, _) = setup(&[]);
    app.settings.network = false;
    open_catalogs(&mut app);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::NetworkOff,
            ..
        }
    ));
    tap(&mut app, (10, 10));
    assert!(matches!(app.view, View::Library { .. }));
}

// --- İndirme ---

const BOOKS_FEED: &str = r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Kitaplar</title>
  <entry><title>Çalıkuşu</title><author><name>Reşat Nuri</name></author>
    <link rel="http://opds-spec.org/acquisition" type="application/epub+zip" href="/kitap.epub"/></entry>
  <entry><title>Kayıp</title>
    <link rel="http://opds-spec.org/acquisition" type="application/epub+zip" href="/yok.epub"/></entry>
  <entry><title>Yavaş</title>
    <link rel="http://opds-spec.org/acquisition" type="application/epub+zip" href="/yavas.epub"/></entry>
</feed>"#;

/// Kitap sunan taklit katalogu açar; katalog sayfasında bırakır.
fn books_catalog(app: &mut App) {
    let book = epub3_sample();
    let url = crate::net::testserver::serve(move |path, _| match path {
        "/opds" => (200, BOOKS_FEED.as_bytes().to_vec()),
        "/kitap.epub" => (200, book.clone()),
        "/yavas.epub" => {
            std::thread::sleep(std::time::Duration::from_millis(800));
            (200, book.clone())
        }
        _ => (404, Vec::new()),
    });
    app.browser.catalogs = vec![local_catalog(format!("{url}/opds"))];
    open_catalogs(app);
    tap_row(app, &catalog::CATALOGS, 0);
    settle(app);
    assert!(matches!(app.view, View::Feed));
}

fn entry_action(app: &App, index: usize) -> downloads::Action {
    app.entry_action(index).unwrap().0
}

/// Kitabın ayrıntısını açıp düğmesine basar ve katalog sayfasına döner.
fn press_entry(app: &mut App, index: usize) {
    tap_row(app, &catalog::FEED, index);
    assert!(matches!(app.view, View::Entry { .. }));
    tap(app, app.entry_button().center());
    tap(app, super::ui::back_button(W).center());
    assert!(matches!(app.view, View::Feed));
}

/// `cond` sağlanana kadar `Tick` gönderir.
fn tick_until(app: &mut App, cond: impl Fn(&App) -> bool) {
    for _ in 0..2000 {
        if cond(app) {
            return;
        }
        app.handle(Event::Tick);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("beklenen duruma gelinmedi");
}

#[test]
fn a_book_is_downloaded_added_to_the_library_and_opened() {
    let (mut app, _, root) = setup(&[]);
    books_catalog(&mut app);
    assert_eq!(entry_action(&app, 0), downloads::Action::Download);
    press_entry(&mut app, 0);
    assert_eq!(app.downloads.len(), 1);
    assert!(app.wants_ticks());
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    settle(&mut app);

    let dest = root.join("İndirilenler").join("Reşat Nuri - Çalıkuşu.epub");
    assert_eq!(std::fs::read(&dest).unwrap(), epub3_sample());
    assert!(
        app.library.iter().any(|e| e.path == dest),
        "kütüphane yeniden tarandı"
    );
    assert!(matches!(app.downloads[0].state, downloads::State::Done));

    // Aynı kitap yeniden indirilmez; "Aç" ile okunur.
    assert_eq!(entry_action(&app, 0), downloads::Action::Open);
    tap_row(&mut app, &catalog::FEED, 0);
    app.draw(&mut c);
    let open = app.entry_button().center();
    tap(&mut app, open);
    assert!(matches!(app.view, View::Contents { .. }));
    assert_eq!(app.downloads.len(), 1);
}

#[test]
fn downloads_queue_fail_retry_and_cancel_without_leftovers() {
    let (mut app, _, root) = setup(&[]);
    books_catalog(&mut app);
    press_entry(&mut app, 1);
    press_entry(&mut app, 2);
    let running = |app: &App| {
        app.downloads
            .iter()
            .filter(|d| matches!(d.state, downloads::State::Running { .. }))
            .count()
    };
    assert_eq!(running(&app), 1, "birer birer iner");

    tick_until(&mut app, |a| {
        matches!(a.downloads[0].state, downloads::State::Failed(_))
    });
    let (action, _, error) = app.entry_action(1).unwrap();
    assert_eq!(action, downloads::Action::Retry);
    assert_eq!(error, Some(Msg::NetStatus(404)));
    assert_eq!(running(&app), 1, "sıradaki başladı");

    // İndirmeler ekranı: katalog başlığındaki düğmeden.
    let button = app.downloads_button(super::ui::back_button(W)).unwrap();
    tap(&mut app, button.center());
    assert!(matches!(app.view, View::Downloads { .. }));
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);

    // Yavaş indirme iptal edilir: listeden çıkar, yarım dosya kalmaz.
    let cancel = app.row_button(1).center();
    tap(&mut app, cancel);
    assert_eq!(app.downloads.len(), 1);
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let dir = root.join("İndirilenler");
    let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
    assert!(left.is_empty(), "kalan dosyalar: {left:?}");

    // Hatalı olan yeniden denenir (yine 404).
    let retry = app.row_button(0).center();
    tap(&mut app, retry);
    assert!(app.wants_ticks());
    settle(&mut app);
    assert!(matches!(
        app.downloads[0].state,
        downloads::State::Failed(_)
    ));
    app.draw(&mut c);
    tap(&mut app, super::ui::back_button(W).center());
    assert!(matches!(app.view, View::Feed));
}

// --- Çevrimiçi sözlük ---

/// Taklit TDK ve Vikisözlük: yalnızca "ağaç" var; "gölge" içeren sorgu geç yanıtlanır.
/// Gelen istekleri sayar.
fn online_dictionary_server() -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let count = std::sync::Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    let agac = "a%C4%9Fa%C3%A7";
    let url = crate::net::testserver::serve(move |path, _| {
        seen.fetch_add(1, Ordering::SeqCst);
        if path.contains("g%C3%B6lge") {
            std::thread::sleep(std::time::Duration::from_millis(800));
        }
        let body = if let Some(q) = path.strip_prefix("/gts?ara=") {
            if q == agac {
                r#"[{"madde":"ağaç","anlamlarListe":[{"anlam_sira":"1","anlam":"Gövdeli bitki"}]}]"#
                    .to_string()
            } else {
                r#"{"error":"Sonuç bulunamadı"}"#.to_string()
            }
        } else if path.contains("prop=categories") {
            if path.contains(agac) {
                r#"{"query":{"pages":[{"title":"ağaçlar"},{"title":"ağaç","categories":[{"title":"Kategori:Türkçe sözcükler"}]}]}}"#.to_string()
            } else {
                r#"{"query":{"pages":[]}}"#.to_string()
            }
        } else {
            r#"{"query":{"pages":[{"title":"ağaç","extract":"== Türkçe ==\n=== Ad ===\nKökü toprakta olan bitki.\n"}]}}"#.to_string()
        };
        (200, body.into_bytes())
    });
    (url, count)
}

#[test]
fn words_missing_locally_are_looked_up_online_and_cached() {
    use std::sync::atomic::Ordering;
    let (url, requests) = online_dictionary_server();
    let (mut app, data, _) = setup(&[("uzun.epub", long_book())]);
    app.defaults.online_tdk = true;
    app.defaults.online_wiktionary = true;
    app.endpoints = super::lookup::Endpoints {
        tdk: format!("{url}/gts"),
        wiktionary: format!("{url}/api.php"),
        wikipedia: url.clone(),
        deepl: None,
        releases: "http://127.0.0.1:1/releases".into(),
    };
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);

    select_and(&mut app, "ağaçlar", 2);
    assert!(matches!(app.view, View::Searching) && app.wants_ticks());
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    settle(&mut app);
    assert!(matches!(app.view, View::Lookup { .. }));
    assert_eq!(app.lookup.as_ref().unwrap().form, "ağaç", "kök bulundu");
    assert!(!app.lookup.as_ref().unwrap().pages[0].lines.is_empty());
    let cache = data.join("cevrimici-sozluk");
    let tdk = std::fs::read_to_string(cache.join("tdk/ağaçlar.json")).unwrap();
    assert!(tdk.contains("Gövdeli bitki"), "{tdk}");
    let wiki = std::fs::read_to_string(cache.join("vikisozluk/ağaçlar.json")).unwrap();
    assert!(wiki.contains("Kökü toprakta olan bitki."), "{wiki}");
    // Önce Vikisözlük (kategori + madde); bulduğu kök "ağaç" TDK'da ilk denenir.
    assert_eq!(requests.load(Ordering::SeqCst), 3);

    // İkinci bakış önbellekten: istek gitmez.
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2));
    select_and(&mut app, "ağaçlar", 2);
    settle(&mut app);
    assert!(matches!(app.view, View::Lookup { .. }));
    assert_eq!(requests.load(Ordering::SeqCst), 3);

    // Yalnızca TDK açıkken kökler sırayla denenir ("gölgeler" yok, sonra "gölge"...).
    app.settings.online_wiktionary = false;
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2));
    select_and(&mut app, "çay", 2);
    settle(&mut app);
    assert!(matches!(&app.view, View::Message { msg: Msg::NotInDictionary(w), .. } if w == "çay"));
    assert_eq!(requests.load(Ordering::SeqCst), 4, "tek aday: çay");
    tap(&mut app, (10, 10));
    app.settings.online_wiktionary = true;

    // Hiçbir yerde olmayan kelime.
    select_and(&mut app, "paragraf", 2);
    settle(&mut app);
    assert!(matches!(
        &app.view,
        View::Message { msg: Msg::NotInDictionary(w), .. } if w == "paragraf"
    ));
    tap(&mut app, (10, 10));

    // Beklerken vazgeçilir.
    select_and(&mut app, "gölgeler", 2);
    assert!(matches!(app.view, View::Searching));
    let cancel = app.loading_cancel().center();
    tap(&mut app, cancel);
    assert!(matches!(app.view, View::Reading) && !app.wants_ticks());

    // Sunucuya ulaşılamıyor: neden açıklanır.
    app.endpoints.tdk = "http://127.0.0.1:1/gts".into();
    app.endpoints.wiktionary = "http://127.0.0.1:1/api.php".into();
    select_and(&mut app, "ışık", 2);
    settle(&mut app);
    let View::Message { msg, .. } = &app.view else {
        panic!("mesaj bekleniyordu");
    };
    assert!(
        matches!(msg, Msg::OnlineFailed { word, .. } if word == "ışık"),
        "{msg:?}"
    );
    app.draw(&mut c);
    tap(&mut app, (10, 10));

    // İnternet kapalıyken çevrimiçi sözlüğe gidilmez.
    app.settings.network = false;
    select_and(&mut app, "ışık", 2);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::NoDictionaries,
            ..
        }
    ));
}

#[test]
fn exit_button_saves_and_asks_the_platform_to_quit() {
    let (mut app, data, root) = reading_long_book();
    key(&mut app, true);
    key(&mut app, true);
    menu(&mut app, MenuItem::Library);
    assert!(!app.wants_exit());
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    tap(&mut app, super::screens::exit_button(W).center());
    assert!(app.wants_exit());
    let mut app = restart(&data, &root);
    tap_row(&mut app, &LIBRARY, 0);
    assert_eq!(session(&app).page, 2, "çevrilen sayfalar kaydedildi");
}

#[test]
fn tap_zone_presets_turn_pages_and_open_the_menu() {
    use super::settings::TapZones;
    let (mut app, _, _) = reading_long_book();
    let h = LIBRA2_HEIGHT;
    let page = |app: &App| session(app).page;

    // Ters: sol ileri, sağ geri.
    app.settings.tap_zones = TapZones::Reversed;
    tap(&mut app, (50, h / 2));
    assert_eq!(page(&app), 1);
    tap(&mut app, (W - 50, h / 2));
    assert_eq!(page(&app), 0);

    // Tek el: ortaya dokunmak ileri, sol şerit geri, üst şerit menü.
    app.settings.tap_zones = TapZones::OneHand;
    tap(&mut app, (W / 2, h / 2));
    tap(&mut app, (W - 50, h - 300));
    assert_eq!(page(&app), 2);
    tap(&mut app, (50, h / 2));
    assert_eq!(page(&app), 1);
    tap(&mut app, (W / 2, 100));
    assert!(matches!(app.view, View::Menu));
}

#[test]
fn reading_pace_is_learned_and_shows_time_left() {
    use std::time::{Duration, Instant};
    let (mut app, data, root) = reading_long_book();
    let read_for = |app: &mut App, secs: u64| {
        app.page_shown = Some(Instant::now() - Duration::from_secs(secs));
        key(app, true);
    };
    assert_eq!(app.time_left(), None, "henüz ölçüm yok");
    read_for(&mut app, 1); // atlama: sayılmaz
    read_for(&mut app, 3600); // ara verme: sayılmaz
    assert_eq!(app.pace.text_samples, 0);
    for _ in 0..3 {
        read_for(&mut app, 60);
    }
    let (chapter, book) = app.time_left().expect("üç ölçümden sonra tahmin var");
    assert!(chapter >= 1 && book >= chapter, "{chapter} {book}");
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);

    // Menüde beklemek okuma sayılmaz.
    middle(&mut app);
    assert_eq!(app.page_shown, None);
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2 + 300));
    key(&mut app, true);
    assert_eq!(app.pace.text_samples, 3);

    // Hız kitaplar arası saklanır.
    app.shutdown();
    let app = restart(&data, &root);
    assert_eq!(app.pace.text_samples, 3);
}

#[test]
fn font_weight_darkens_book_text_but_not_the_interface() {
    let (mut app, _, _) = reading_long_book();
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    let ink_of = |app: &mut App, c: &mut Canvas| {
        app.draw(c);
        ink(c)
    };
    let thin = ink_of(&mut app, &mut c);
    let mut bold = app.settings.clone();
    bold.weight = 3;
    app.apply_settings(bold);
    let thick = ink_of(&mut app, &mut c);
    assert!(thick > thin + thin / 20, "{thin} → {thick}");

    menu(&mut app, MenuItem::Library);
    let ui_bold = ink_of(&mut app, &mut c);
    let mut normal = app.settings.clone();
    normal.weight = 0;
    app.apply_settings(normal);
    assert_eq!(ink_of(&mut app, &mut c), ui_bold, "arayüz yazısı değişmez");
}

#[test]
fn text_settings_belong_to_each_book_until_made_default() {
    let (mut app, data, root) = setup(&[("a.epub", long_book()), ("b.epub", epub3_sample())]);
    // `row`: kitabın `library` sırası; kütüphanede görünen satırı sıralamaya göre değişir.
    let open = |app: &mut App, row: usize| {
        if app.session.is_some() {
            menu(app, MenuItem::Library);
        }
        let shown = app.visible_books().iter().position(|&i| i == row).unwrap();
        tap_row(app, &LIBRARY, shown);
        if matches!(app.view, View::Contents { .. }) {
            tap_row(app, &CONTENTS, 0);
        }
        assert!(matches!(app.view, View::Reading));
    };
    let a_row = app
        .library
        .iter()
        .position(|e| e.path.ends_with("a.epub"))
        .unwrap();
    let b_row = 1 - a_row;

    // A'da büyük yazı ve gece modu.
    open(&mut app, a_row);
    change_setting(&mut app, Field::Size, 2);
    menu(&mut app, MenuItem::Settings);
    press_plus(&mut app, Field::Night);
    let at = app.settings_ok().center();
    tap(&mut app, at);
    assert_eq!(app.settings().font_px, 48.0);

    // B varsayılan boyutta; gece modu genel ayar olduğundan onda da açık.
    open(&mut app, b_row);
    assert_eq!(app.settings().font_px, 40.0);
    assert!(app.settings().night);

    // B'de küçük yazıyı varsayılan yap: yeni kitaplar onu alır, A kendi boyutunu korur.
    menu(&mut app, MenuItem::Settings);
    let size_row = FIELD_PAGES[0]
        .iter()
        .position(|f| *f == Field::Size)
        .unwrap();
    let minus = app.settings_buttons(size_row).0.center();
    tap(&mut app, minus);
    let at = app.settings_default_button().center();
    tap(&mut app, at);
    assert_eq!(app.settings().font_px, 36.0);
    assert!(session(&app).state.layout.is_none());

    let mut app = restart(&data, &root);
    assert_eq!(app.settings().font_px, 36.0, "varsayılan kalıcı");
    open(&mut app, a_row);
    assert_eq!(app.settings().font_px, 48.0, "A kendi ayarını korur");
    open(&mut app, b_row);
    assert_eq!(app.settings().font_px, 36.0);
}

// --- Bağlantılar ---

fn linked_book() -> Vec<u8> {
    let filler =
        "<p>Dolgu paragrafı: sayfaları doldurmak için yazılmış uzunca bir cümle.</p>".repeat(60);
    let one = format!(
        r##"<html><body><p>Bkz. <a href="c2.xhtml#hedef">ikinci</a> bölüm, <a href="#asagi">aşağısı</a> ve <a href="https://ornek.org">dışarısı</a>.</p>{filler}<p id="asagi">Aşağıdaki paragraf.</p></body></html>"##
    );
    let two =
        format!(r#"<html><body>{filler}<h2 id="hedef">Hedef başlık</h2><p>Son.</p></body></html>"#);
    let opf = opf_doc(
        r#"<item id="a" href="c1.xhtml" media-type="application/xhtml+xml"/><item id="b" href="c2.xhtml" media-type="application/xhtml+xml"/>"#,
        r#"<itemref idref="a"/><itemref idref="b"/>"#,
    );
    zip_of(&[
        ("mimetype", "application/epub+zip"),
        ("META-INF/container.xml", &container("OEBPS/content.opf")),
        ("OEBPS/content.opf", &opf),
        ("OEBPS/c1.xhtml", &one),
        ("OEBPS/c2.xhtml", &two),
    ])
}

/// Okunan sayfada `anchor` kimliğini taşıyan blok var mı?
fn page_shows_anchor(app: &App, anchor: &str) -> bool {
    let s = session(app);
    s.pages[s.page]
        .lines
        .iter()
        .any(|l| s.blocks[l.block].anchors.iter().any(|a| a == anchor))
}

#[test]
fn links_jump_to_their_targets_and_back() {
    let (mut app, _, _) = setup(&[("baglanti.epub", linked_book())]);
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    assert_eq!(session(&app).spine, 0);
    assert!(app.link_back_button().is_none());

    // Başka bölümdeki kimlik.
    let at = word_point(&app, "ikinci");
    tap(&mut app, at);
    assert_eq!(session(&app).spine, 1);
    assert!(session(&app).page > 0, "hedef bölümün başında değil");
    assert!(page_shows_anchor(&app, "hedef"));
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    let back = app.link_back_button().expect("geri düğmesi görünür");
    tap(&mut app, back.center());
    assert_eq!((session(&app).spine, session(&app).page), (0, 0));
    assert!(app.link_back_button().is_none());

    // Aynı bölümün ilerisi.
    let at = word_point(&app, "aşağısı");
    tap(&mut app, at);
    assert_eq!(session(&app).spine, 0);
    assert!(page_shows_anchor(&app, "asagi"));
    let back = app.link_back_button().unwrap().center();
    tap(&mut app, back);
    assert_eq!(session(&app).page, 0);

    // Dış bağlantı izlenmez.
    let at = word_point(&app, "dışarısı");
    tap(&mut app, at);
    assert_eq!(session(&app).spine, 0);
    assert!(app.link_back_button().is_none());
}

fn book_with_notes() -> Vec<u8> {
    let one = r##"<html><body><p>Metin<a href="notlar.xhtml#n1" epub:type="noteref">1</a> ve ikinci gönderme <a href="notlar.xhtml#n2">[2]</a>; ayrıca <a href="notlar.xhtml#n3">notların hepsi</a>.</p></body></html>"##;
    let notes = r#"<html><body><aside id="n1"><p>Birinci not.</p><p>Devamı.</p></aside><p id="n2">İkinci not.</p><p id="n3">Üçüncü.</p></body></html>"#;
    let opf = opf_doc(
        r#"<item id="a" href="c1.xhtml" media-type="application/xhtml+xml"/><item id="b" href="notlar.xhtml" media-type="application/xhtml+xml"/>"#,
        r#"<itemref idref="a"/><itemref idref="b"/>"#,
    );
    zip_of(&[
        ("mimetype", "application/epub+zip"),
        ("META-INF/container.xml", &container("OEBPS/content.opf")),
        ("OEBPS/content.opf", &opf),
        ("OEBPS/c1.xhtml", one),
        ("OEBPS/notlar.xhtml", notes),
    ])
}

#[test]
fn footnotes_open_in_a_popup_without_leaving_the_page() {
    let (mut app, _, _) = setup(&[("notlu.epub", book_with_notes())]);
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);

    // İşaretli dipnot: <aside> içindeki iki paragraf.
    let at = word_point(&app, "1");
    tap(&mut app, at);
    assert!(matches!(app.view, View::Footnote));
    assert_eq!(app.footnote.as_ref().unwrap().text, "Birinci not.\nDevamı.");
    app.draw(&mut c);
    tap(&mut app, (W / 2, 200)); // pencerenin dışı
    assert!(matches!(app.view, View::Reading));
    assert_eq!(
        (session(&app).spine, session(&app).page),
        (0, 0),
        "sayfa değişmedi"
    );

    // İşaretsiz ama kısa metin de dipnot; "Notun yerine git" nota gider.
    let at = word_point(&app, "[2]");
    tap(&mut app, at);
    assert_eq!(app.footnote.as_ref().unwrap().text, "İkinci not.");
    let (_, _, go) = app.footnote_layout().unwrap();
    tap(&mut app, go.center());
    assert_eq!(session(&app).spine, 1);
    assert!(app.link_back_button().is_some());
    let back = app.link_back_button().unwrap().center();
    tap(&mut app, back);

    // Uzun metinli bağlantı dipnot sayılmaz, izlenir.
    let at = word_point(&app, "notların");
    tap(&mut app, at);
    assert!(matches!(app.view, View::Reading));
    assert_eq!(session(&app).spine, 1);
}

// --- Kitap içi arama ---

fn search_for(app: &mut App, query: &str) {
    if !matches!(app.view, View::SearchResults { .. }) {
        menu(app, MenuItem::Search);
    }
    if matches!(app.view, View::SearchResults { .. }) {
        tap(app, super::ui::header_button(W, 260).center()); // Yeni arama
    }
    let (count, del) = {
        let View::Form(f) = &app.view else {
            panic!("arama formu açık değil");
        };
        let del = f
            .keyboard
            .keys(W)
            .into_iter()
            .find(|(k, _)| *k == keyboard::Key::Backspace)
            .unwrap()
            .1
            .center();
        (f.fields[0].value.chars().count(), del)
    };
    for _ in 0..count {
        tap(app, del);
    }
    type_text(app, query);
    form_button(app, form::Button::Submit);
    assert!(matches!(app.view, View::SearchResults { page: 0 }));
}

#[test]
fn book_search_lists_hits_and_jumps_to_them() {
    let (mut app, _, _) = setup(&[("baglanti.epub", linked_book())]);
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);

    search_for(&mut app, "hedef");
    let hits = &app.search.as_ref().unwrap().hits;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].snippet, "Hedef başlık");
    app.draw(&mut c);

    tap_row(&mut app, &super::screens::SEARCH, 0);
    assert!(matches!(app.view, View::Reading));
    assert_eq!(session(&app).spine, 1);
    assert!(page_shows_anchor(&app, "hedef"), "sonucun sayfası açıldı");
    app.draw(&mut c);
    let results = app.results_button().expect("sonuçlara dönüş düğmesi");
    tap(&mut app, results.center());
    assert!(matches!(app.view, View::SearchResults { .. }));
    tap(&mut app, super::ui::back_button(W).center());
    assert!(matches!(app.view, View::Reading));
    assert!(app.results_button().is_none());

    // İki bölüm, büyük harfle başlayan kelime: her bölümde 60 dolgu paragrafı.
    search_for(&mut app, "dolgu");
    let hits = &app.search.as_ref().unwrap().hits;
    assert_eq!(hits.len(), 120);
    assert_eq!(hits.iter().filter(|h| h.spine == 1).count(), 60);

    search_for(&mut app, "bulunmayan");
    assert!(app.search.as_ref().unwrap().hits.is_empty());
    app.draw(&mut c);
}

#[test]
fn pdf_pages_are_searched_too() {
    let (mut app, _, _) = setup(&[("metin.pdf", text_pdf())]);
    tap_row(&mut app, &LIBRARY, 0);
    search_for(&mut app, "ikinci");
    let hits = &app.search.as_ref().unwrap().hits;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].spine, 1);
    tap_row(&mut app, &super::screens::SEARCH, 0);
    assert_eq!(session(&app).spine, 1, "ikinci sayfa açıldı");
    assert!(app.search_mark.is_some());
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
}

// --- Kaydırma ve iki parmak ---

fn swipe(app: &mut App, from: (u32, u32), to: (u32, u32)) {
    use crate::event::TouchPhase::*;
    for (phase, (x, y), ms) in [(Down, from, 0), (Move, to, 100), (Up, to, 200)] {
        app.handle(Event::Touch { phase, x, y, ms });
    }
}

fn pinch(app: &mut App, from: u32, to: u32) {
    use crate::event::TouchPhase::*;
    let (cx, y) = (W / 2, LIBRA2_HEIGHT / 2);
    app.handle(Event::Touch {
        phase: Down,
        x: cx - from / 2,
        y,
        ms: 0,
    });
    app.handle(Event::SecondTouch {
        phase: Down,
        x: cx + from / 2,
        y,
    });
    app.handle(Event::Touch {
        phase: Move,
        x: cx - to / 2,
        y,
        ms: 100,
    });
    app.handle(Event::SecondTouch {
        phase: Up,
        x: cx + to / 2,
        y,
    });
    app.handle(Event::Touch {
        phase: Up,
        x: cx - to / 2,
        y,
        ms: 200,
    });
}

#[test]
fn swipes_turn_pages_and_pinches_change_the_font_size() {
    let (mut app, _, _) = reading_long_book();
    swipe(&mut app, (900, 800), (300, 820));
    assert_eq!(session(&app).page, 1, "sola kaydırma ileri");
    swipe(&mut app, (300, 800), (900, 790));
    assert_eq!(session(&app).page, 0, "sağa kaydırma geri");
    swipe(&mut app, (600, 300), (620, 1200));
    assert_eq!(session(&app).page, 0, "dikey kaydırma çevirmez");
    assert!(matches!(app.view, View::Reading));

    pinch(&mut app, 200, 400);
    assert_eq!(app.settings().font_px, 44.0, "açılınca büyür");
    pinch(&mut app, 400, 200);
    pinch(&mut app, 400, 200);
    assert_eq!(app.settings().font_px, 36.0, "kapanınca küçülür");
    pinch(&mut app, 300, 310);
    assert_eq!(app.settings().font_px, 36.0, "küçük oynama değiştirmez");
    assert!(matches!(app.view, View::Reading), "iki parmak menü açmaz");
}

// --- Görseller ve tablolar ---

fn illustrated_book() -> Vec<u8> {
    let cover = r#"<html><body><svg xmlns:xlink="http://www.w3.org/1999/xlink"><image xlink:href="kapak.png"/></svg></body></html>"#;
    let body = r#"<html><body><p>Önce.</p><p><img src="img/resim.png" alt="Bir resim"/></p><p>Sonra.</p>
        <img src="img/hareketli.gif"/>
        <table><tr><th>Ad</th><th>Yaş</th></tr><tr><td>Ali</td><td>30</td></tr></table></body></html>"#;
    let opf = opf_doc(
        r#"<item id="k" href="kapak.xhtml" media-type="application/xhtml+xml"/><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/>"#,
        r#"<itemref idref="k"/><itemref idref="c"/>"#,
    );
    let (cover_png, picture) = (
        crate::epub::testutil::png(600, 900, 40),
        crate::epub::testutil::png(200, 100, 60),
    );
    crate::epub::testutil::zip_of_bytes(&[
        ("mimetype", b"application/epub+zip"),
        (
            "META-INF/container.xml",
            container("OEBPS/content.opf").as_bytes(),
        ),
        ("OEBPS/content.opf", opf.as_bytes()),
        ("OEBPS/kapak.xhtml", cover.as_bytes()),
        ("OEBPS/c.xhtml", body.as_bytes()),
        ("OEBPS/kapak.png", &cover_png),
        ("OEBPS/img/resim.png", &picture),
        ("OEBPS/img/hareketli.gif", b"GIF89a bozuk"),
    ])
}

#[test]
fn images_are_parsed_sized_and_tables_flow_as_rows() {
    let mut book = crate::epub::testutil::open_bytes(illustrated_book()).unwrap();
    let cover = book.chapter(0).unwrap();
    let img = cover[0].image.as_ref().expect("SVG içindeki kapak");
    assert_eq!(
        (img.path.as_str(), img.width, img.height),
        ("OEBPS/kapak.png", 600, 900)
    );

    let blocks = book.chapter(1).unwrap();
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    let picture = blocks.iter().find(|b| b.image.is_some()).unwrap();
    assert_eq!(picture.text, "Bir resim", "alt metin");
    assert_eq!(picture.image.as_ref().unwrap().width, 200);
    assert!(texts.contains(&"[Görsel gösterilemiyor]"), "GIF: {texts:?}");
    assert!(
        texts.contains(&"Ad | Yaş") && texts.contains(&"Ali | 30"),
        "{texts:?}"
    );
    let header = blocks.iter().find(|b| b.text == "Ad | Yaş").unwrap();
    assert!(
        header
            .spans
            .iter()
            .all(|s| s.style.bold || header.text[s.start..s.end].trim() == "|")
    );
}

#[test]
fn images_are_drawn_and_open_full_screen_on_hold() {
    let (mut app, _, _) = setup(&[("resimli.epub", illustrated_book())]);
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    // Kapak: sayfayı kaplayan görsel; ortasında koyu piksel.
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    let mid = (LIBRA2_HEIGHT / 2 * W + W / 2) as usize;
    assert!(c.pixels()[mid] < 80, "kapak çizildi: {}", c.pixels()[mid]);

    // Uzun basma tam ekran açar, dokunma kapatır; kısa dokunma sayfayı çevirir.
    app.handle(Event::Hold {
        x: W / 2,
        y: LIBRA2_HEIGHT / 2,
    });
    assert!(matches!(app.view, View::Image { block: 0 }));
    app.draw(&mut c);
    tap(&mut app, (10, 10));
    assert!(matches!(app.view, View::Reading));
    tap(&mut app, (W - 50, LIBRA2_HEIGHT / 2));
    assert_eq!(session(&app).spine, 1, "kapaktan sonraki bölüm");
    app.draw(&mut c);
    assert!(
        session(&app).pages[0]
            .lines
            .iter()
            .any(|l| l.line.glyphs.is_empty() && l.height == 200),
        "200×100'lük görsel iki kat büyütülüp yerleşti"
    );
}

// --- Yatay okuma ---

#[test]
fn reading_turns_landscape_but_other_screens_stay_portrait() {
    use super::settings::Orientation;
    let (mut app, _, _) = reading_long_book();
    for _ in 0..5 {
        key(&mut app, true);
    }
    let before = session(&app).position();
    let portrait_pages = session(&app).pages.len();
    assert_eq!(app.screen_size(), (W, LIBRA2_HEIGHT));

    let mut wide = app.settings.clone();
    wide.orientation = Orientation::Landscape;
    wide.two_columns = false; // tek geniş sütun (iki sütun ayrı testte)
    app.apply_settings(wide);
    assert!(app.fit_orientation(), "ekran döndü");
    let (w, h) = app.screen_size();
    assert_eq!((w, h), (LIBRA2_HEIGHT, W), "okuma yatay");
    let s = session(&app);
    assert_eq!(s.laid_out, (w, h));
    // Yatayda satırlar dikey metin genişliğinden uzun (sayfa sayısı yaklaşık aynı
    // kalır: satırlar uzar ama sayfaya daha az satır sığar).
    let widest = s
        .pages
        .iter()
        .flat_map(|p| &p.lines)
        .filter_map(|l| l.line.glyphs.last().map(|(g, gx)| gx + g.advance))
        .fold(0.0f32, f32::max);
    let portrait_text = (W as i32 - 2 * app.settings.margin) as f32;
    assert!(
        widest > portrait_text,
        "{widest} ({portrait_pages} dikey sayfa)"
    );
    let bottom = super::ui::bottom_of(h);
    assert!(
        s.pages
            .iter()
            .flat_map(|p| &p.lines)
            .all(|l| l.top + l.height <= bottom + 2)
    );
    assert!(s.pages[s.page].start <= before, "konum korunur");
    assert!(s.pages.get(s.page + 1).is_none_or(|n| before < n.start));
    let mut c = Canvas::new(w, h);
    app.draw(&mut c);

    // Menü yatayda açılır; ayarlar dikeyde, kapatınca yine yatay.
    middle(&mut app);
    assert!(matches!(app.view, View::Menu));
    assert_eq!(app.screen_size(), (w, h));
    app.draw(&mut c);
    let index = app
        .menu_items()
        .iter()
        .position(|m| *m == MenuItem::Settings)
        .unwrap();
    let at = app.menu_rect(index).center();
    tap(&mut app, at);
    assert!(matches!(app.view, View::Settings { .. }));
    assert_eq!(app.screen_size(), (W, LIBRA2_HEIGHT), "ayarlar dikey");
    let cancel = app.settings_cancel().center();
    tap(&mut app, cancel);
    assert_eq!(app.screen_size(), (w, h), "okumaya dönünce yatay");

    // Otomatik: ivmeölçere göre.
    let mut auto = app.settings.clone();
    auto.orientation = Orientation::Auto;
    app.apply_settings(auto);
    app.fit_orientation();
    assert_eq!(app.screen_size(), (W, LIBRA2_HEIGHT), "cihaz dik");
    assert_eq!(
        app.handle(Event::Tilt { landscape: true }),
        Some(Refresh::Full)
    );
    assert_eq!(app.screen_size(), (w, h));
    assert_eq!(
        app.handle(Event::Tilt { landscape: true }),
        None,
        "değişiklik yok"
    );
    menu(&mut app, MenuItem::Library);
    assert_eq!(app.screen_size(), (W, LIBRA2_HEIGHT), "kütüphane dikey");
}

#[test]
fn landscape_epub_reads_in_two_columns() {
    let (mut app, _, _) = reading_long_book();
    for _ in 0..3 {
        key(&mut app, true);
    }
    let before = session(&app).position();
    app.handle(Event::Tilt { landscape: true });
    let (w, h) = app.screen_size();
    let s = session(&app);
    assert_eq!(s.columns, 2);
    assert_eq!(s.page % 2, 0, "sayfa iki sütunun ilki");
    assert!(s.pages[s.page].start <= before && before < s.pages[s.page + 2].start);
    let column_w = (w as i32 - 3 * app.settings.margin) / 2;
    let widest = s
        .pages
        .iter()
        .flat_map(|p| &p.lines)
        .filter_map(|l| l.line.glyphs.last().map(|(g, gx)| gx + g.advance))
        .fold(0.0f32, f32::max);
    assert!(
        widest <= column_w as f32 + 1.0,
        "satırlar sütuna sığar: {widest}"
    );

    // Sağ sütun çizilir ve dokunulabilir.
    let mut c = Canvas::new(w, h);
    app.draw(&mut c);
    let right_ink = (0..h)
        .flat_map(|y| (w / 2 + 40..w - 40).map(move |x| (x, y)))
        .filter(|&(x, y)| c.pixels()[(y * w + x) as usize] < 128)
        .count();
    assert!(right_ink > 1000, "sağ sütunda metin: {right_ink}");
    let s = session(&app);
    let right = &s.pages[s.page + 1].lines[0];
    let y = (right.top + right.height / 2) as u32;
    let word = app.word_at(w / 2 + 120, y).expect("sağ sütunda kelime");
    assert_eq!(word.0.block, right.block);

    // Çevirme iki sütun atlar.
    let page = session(&app).page;
    key(&mut app, true);
    assert_eq!(session(&app).page, page + 2);
    key(&mut app, false);
    assert_eq!(session(&app).page, page);

    // Tek sütun ayarı.
    let mut one = app.settings.clone();
    one.two_columns = false;
    app.apply_settings(one);
    assert_eq!(session(&app).columns, 1);
}

#[test]
fn landscape_pdf_shows_two_pages_side_by_side() {
    let page = |t: &str| format!("BT /F1 40 Tf 72 700 Td ({t}) Tj ET 0 0 0 rg 72 100 450 500 re f");
    let pages = [page("Bir"), page("Iki"), page("Uc")];
    let pdf = crate::pdf::testutil::pdf_with_content(
        &pages.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let (mut app, _, _) = setup(&[("uc.pdf", pdf)]);
    tap_row(&mut app, &LIBRARY, 0);
    assert_eq!(session(&app).spine, 0);
    app.handle(Event::Tilt { landscape: true });
    let (w, h) = app.screen_size();
    let s = session(&app);
    assert!(s.spread && s.right.is_some(), "1. ve 2. sayfa yan yana");

    let mut c = Canvas::new(w, h);
    app.draw(&mut c);
    let ink_in = |c: &Canvas, x0: u32, x1: u32| {
        (0..h)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|&(x, y)| c.pixels()[(y * w + x) as usize] < 100)
            .count()
    };
    assert!(ink_in(&c, 0, w / 2) > 10_000, "sol sayfa");
    assert!(ink_in(&c, w / 2, w) > 10_000, "sağ sayfa");

    key(&mut app, true);
    let s = session(&app);
    assert_eq!(s.spine, 2, "iki sayfa atlandı");
    assert!(s.right.is_none(), "son tek sayfa");
    app.draw(&mut c);
    key(&mut app, false);
    assert_eq!(session(&app).spine, 0);

    // Dikeye dönünce tek sayfa.
    app.handle(Event::Tilt { landscape: false });
    assert!(!session(&app).spread);
}

#[test]
fn page_keys_can_be_swapped() {
    let (mut app, _, _) = reading_long_book();
    key(&mut app, true);
    assert_eq!(session(&app).page, 1);
    app.settings.swap_keys = true;
    key(&mut app, true);
    assert_eq!(session(&app).page, 0, "üst tuş (ileri olayı) artık geri");
    key(&mut app, false);
    assert_eq!(session(&app).page, 1);
}

#[test]
fn auto_turn_turns_pages_until_touched() {
    use std::time::{Duration, Instant};
    let (mut app, _, _) = reading_long_book();
    app.settings.auto_turn = 10;
    menu(&mut app, MenuItem::AutoTurn);
    assert!(matches!(app.view, View::Reading));
    assert!(app.wants_ticks(), "çalışırken Tick ister");
    assert_eq!(app.handle(Event::Tick), None, "süre dolmadı");
    for page in 1..=2 {
        app.auto_turn = Some(Instant::now() - Duration::from_secs(11));
        assert!(app.handle(Event::Tick).is_some());
        assert_eq!(session(&app).page, page);
    }
    assert_eq!(app.pace.text_samples, 0, "otomatik çevirme hıza katılmaz");

    // Dokunma yalnızca durdurur (menü açmaz, sayfa çevirmez).
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2));
    assert!(matches!(app.view, View::Reading));
    assert_eq!(session(&app).page, 2);
    assert!(app.auto_turn.is_none() && !app.wants_ticks());

    // Okuma hızına göre: hız bilinmiyorsa 30 sn.
    app.settings.auto_turn = 0;
    menu(&mut app, MenuItem::AutoTurn);
    app.auto_turn = Some(Instant::now() - Duration::from_secs(20));
    assert_eq!(app.handle(Event::Tick), None);
    app.auto_turn = Some(Instant::now() - Duration::from_secs(31));
    assert!(app.handle(Event::Tick).is_some());
}

// --- Kütüphane araç çubuğu ---

fn shown_titles(app: &App) -> Vec<String> {
    app.visible_books()
        .iter()
        .map(|&i| app.library[i].display_title())
        .collect()
}

#[test]
fn library_is_searched_sorted_and_filtered_by_reading_status() {
    let (mut app, data, root) = setup(&[
        ("a.epub", epub3_sample()),
        ("b.epub", epub2_sample()),
        ("c.epub", long_book()),
    ]);
    let all = shown_titles(&app);
    assert_eq!(all.len(), 3);
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);

    // Son okunan önce.
    let last = all[2].clone();
    tap_row(&mut app, &LIBRARY, 2);
    if session(&app).pages.is_empty() {
        tap_row(&mut app, &CONTENTS, 0);
    }
    menu(&mut app, MenuItem::Library);
    assert_eq!(shown_titles(&app)[0], last);

    // Sıra: başlık (Türk alfabesi).
    let (search, sort, _, filters) = app.tool_rects();
    tap(&mut app, sort.center());
    assert_eq!(app.settings().library_sort, super::shelf::Sort::Title);
    assert_eq!(shown_titles(&app), all);

    // Arama: yazar ya da başlık, büyük/küçük harf fark etmez.
    tap(&mut app, search.center());
    type_text(&mut app, "kuyucak");
    form_button(&mut app, form::Button::Submit);
    assert_eq!(shown_titles(&app), ["Kuyucaklı Yusuf"]);
    app.draw(&mut c);
    tap(&mut app, search.center());
    assert_eq!(shown_titles(&app).len(), 3, "arama kaldırıldı");

    // Süzgeç: okunuyor yalnızca açılan kitap.
    tap(&mut app, filters[1].center());
    assert_eq!(shown_titles(&app), std::slice::from_ref(&last));

    // Uzun basma: okuma durumu; "bitti" süzgecinde görünür, kalıcı.
    let (x, y) = LIBRARY.row_rect(W, 0).center();
    app.handle(Event::Hold { x, y });
    assert!(matches!(app.view, View::BookActions { .. }));
    app.draw(&mut c);
    let finished = app.status_rect(2).center();
    tap(&mut app, finished);
    assert!(shown_titles(&app).is_empty(), "artık okunmuyor");
    tap(&mut app, filters[3].center());
    assert_eq!(shown_titles(&app), std::slice::from_ref(&last));
    app.draw(&mut c);

    let app = restart(&data, &root);
    assert_eq!(shown_titles(&app), [last], "süzgeç ve durum kalıcı");
}

#[test]
fn books_are_deleted_after_confirmation_and_moved_to_folders() {
    let (mut app, data, root) = setup(&[("a.epub", epub3_sample()), ("b.epub", epub2_sample())]);
    std::fs::create_dir_all(root.join("Romanlar")).unwrap();
    let hold = |app: &mut App, row: usize| {
        let (x, y) = LIBRARY.row_rect(W, row).center();
        app.handle(Event::Hold { x, y });
        assert!(matches!(app.view, View::BookActions { .. }));
    };
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);

    // Taşı: ilk kitap "Romanlar"a.
    let first = app.library[app.visible_books()[0]].path.clone();
    hold(&mut app, 0);
    let at = app.status_rect(super::shelf::MOVE_ROW).center();
    tap(&mut app, at);
    assert!(matches!(app.view, View::MoveBook { .. }));
    app.draw(&mut c);
    let folders = crate::library::folders(&root);
    let target = folders
        .iter()
        .position(|f| f.ends_with("Romanlar"))
        .unwrap();
    tap_row(&mut app, &super::shelf::FOLDERS, target);
    let moved = root.join("Romanlar").join(first.file_name().unwrap());
    assert!(moved.exists() && !first.exists());
    assert!(
        app.library.iter().any(|e| e.path == moved),
        "kütüphane yeniden tarandı"
    );

    // Sil: onaydan "Vazgeç" dosyayı bırakır, "Sil" siler; kayıt kalır.
    let victim = app.library[app.visible_books()[0]].clone();
    tap_row(&mut app, &LIBRARY, 0);
    if session(&app).pages.is_empty() {
        tap_row(&mut app, &CONTENTS, 0);
    }
    menu(&mut app, MenuItem::Library);
    let row = app
        .visible_books()
        .iter()
        .position(|&i| app.library[i].id == victim.id)
        .unwrap();
    hold(&mut app, row);
    let at = app.status_rect(super::shelf::DELETE_ROW).center();
    tap(&mut app, at);
    assert!(matches!(app.view, View::ConfirmDelete { .. }));
    app.draw(&mut c);
    let cancel = app.status_rect(1).center();
    tap(&mut app, cancel);
    assert!(victim.path.exists());
    hold(&mut app, row);
    let at = app.status_rect(super::shelf::DELETE_ROW).center();
    tap(&mut app, at);
    let delete = app.status_rect(0).center();
    tap(&mut app, delete);
    assert!(!victim.path.exists());
    assert_eq!(app.library.len(), 1);
    assert!(
        data.join(format!("books/{}.json", victim.id)).exists(),
        "notlar kayıtta kalır"
    );
}

#[test]
fn word_list_exports_anki_cards_with_definitions() {
    let (mut app, _, root) = setup(&[("uzun.epub", long_book())]);
    let dir = root.join("Sözlükler");
    std::fs::create_dir_all(&dir).unwrap();
    crate::dict::testutil::stardict(
        &dir,
        "deneme",
        &[("ağaç", "Gövdeli bitki; odunsu.")],
        &[],
        false,
    );
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    select_and(&mut app, "ağaçlar", 2);
    let at = app.word_button().center();
    tap(&mut app, at);
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2));
    menu(&mut app, MenuItem::Words);
    tap(&mut app, super::ui::header_button(W, 280).center());
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::AnkiExported(1),
            ..
        }
    ));
    let csv = std::fs::read_to_string(root.join(super::words::ANKI_FILE)).unwrap();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "#separator:Semicolon");
    assert_eq!(
        lines[3], "ağaç;\"Gövdeli bitki; odunsu. — “ağaçlar” · Deneme\"",
        "noktalı virgüllü alan tırnaklanır"
    );
}

#[test]
fn annotations_are_searched_in_quotes_and_notes() {
    let (mut app, _, _) = reading_long_book();
    select_and(&mut app, "ağaçlar", 0); // Vurgula
    select_and(&mut app, "gölgeler", 0);
    let s = app.session.as_mut().unwrap();
    assert_eq!(s.state.annotations.len(), 2);
    let i = s
        .state
        .annotations
        .iter()
        .position(|a| a.quote == "gölgeler")
        .unwrap();
    s.set_note(i, Some("Işıkla karşıtlık".into()));
    menu(&mut app, MenuItem::Annotations);
    let rows = |app: &App| app.annotation_rows().len();
    assert_eq!(rows(&app), 2);

    let search = |app: &mut App, q: &str| {
        let at = app.annotation_search_button().center();
        tap(app, at);
        type_text(app, q);
        form_button(app, form::Button::Submit);
        assert!(matches!(app.view, View::Annotations { .. }));
    };
    search(&mut app, "ağaç");
    assert_eq!(rows(&app), 1, "alıntıda");
    let clear = app.annotation_search_button().center();
    tap(&mut app, clear);
    search(&mut app, "ışık");
    assert_eq!(rows(&app), 1, "notta");
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    tap(&mut app, clear);
    search(&mut app, "yok");
    assert_eq!(rows(&app), 0);
    app.draw(&mut c);
    tap(&mut app, clear);
    assert_eq!(rows(&app), 2, "arama kaldırıldı");
}

#[test]
fn selected_text_is_looked_up_on_wikipedia() {
    let url = crate::net::testserver::serve(|path, _| {
        let body = if path.contains("list=search") {
            if path.contains("bulunmayan") {
                r#"{"query":{"search":[]}}"#.to_string()
            } else {
                r#"{"query":{"search":[{"title":"Ağaç"}]}}"#.to_string()
            }
        } else if path.starts_with("/api/rest_v1/page/summary/A%C4%9Fa%C3%A7") {
            r#"{"title":"Ağaç","extract":"Ağaç, odunsu gövdeli bitkidir."}"#.to_string()
        } else {
            String::new()
        };
        (200, body.into_bytes())
    });
    let (mut app, _, _) = reading_long_book();
    app.endpoints.wikipedia = url;
    select_and(&mut app, "ağaçlar", 3); // Vikipedi
    assert!(matches!(app.view, View::Searching));
    settle(&mut app);
    assert!(matches!(app.view, View::Lookup { .. }));
    assert_eq!(app.lookup.as_ref().unwrap().form, "Ağaç");
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2));

    // Bulunamayan ve internet kapalı.
    app.look_up_wikipedia("bulunmayan");
    settle(&mut app);
    assert!(
        matches!(&app.view, View::Message { msg: Msg::NotOnWikipedia(w), .. } if w == "bulunmayan")
    );
    tap(&mut app, (10, 10));
    app.settings.network = false;
    select_and(&mut app, "gölgeler", 3);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::NetworkOff,
            ..
        }
    ));
}

#[test]
fn cover_grid_draws_covers_in_background_and_opens_books() {
    let (mut app, data, root) = setup(&[
        ("a.epub", epub3_sample()),
        ("k.epub", epub_with_cover(true)),
        ("z.pdf", pdf_book()),
    ]);
    let (_, _, view, _) = app.tool_rects();
    tap(&mut app, view.center());
    assert_eq!(
        app.settings().library_view,
        super::shelf::LibraryView::Covers
    );
    let job = app
        .cover_job
        .take()
        .expect("kapaklar arka planda çiziliyor");
    let done = job.wait().unwrap();
    assert_eq!(done.len(), 3);
    app.covers.extend(done);
    let has_cover = |app: &App, title: &str| {
        let e = app
            .library
            .iter()
            .find(|e| e.display_title() == title)
            .unwrap();
        app.covers[&e.id].is_some()
    };
    assert!(has_cover(&app, "Kapaklı"), "EPUB'ın kendi kapağı");
    assert!(has_cover(&app, "z"), "PDF'in ilk sayfası");
    assert!(!has_cover(&app, "Çalıkuşu"), "kapağı yok");
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);

    // İkinci hücre: Kapaklı (Türkçe sıra: Çalıkuşu, Kapaklı, z).
    let titles = shown_titles(&app);
    assert_eq!(titles[1], "Kapaklı");
    let second = (W / 2, LIBRARY.top as u32 + 100);
    app.handle(Event::Hold {
        x: second.0,
        y: second.1,
    });
    assert!(
        matches!(app.view, View::BookActions { book } if app.library[book].display_title() == "Kapaklı")
    );
    tap(&mut app, (10, 10));
    tap(&mut app, second);
    assert_eq!(session(&app).name, "k");

    // Yeniden açılışta kapaklar diskten gelir, yeniden çizilmez.
    let mut app = restart(&data, &root);
    app.start();
    assert!(app.cover_job.is_none());
    assert!(has_cover(&app, "Kapaklı"));
    assert!(!has_cover(&app, "Çalıkuşu"));
}

#[test]
fn reading_time_and_pages_are_counted_saved_and_shown() {
    let (mut app, data, root) = reading_long_book();
    let back = |secs| Some(std::time::Instant::now() - std::time::Duration::from_secs(secs));
    // İleri çevrilen sayfa: süre ve sayfa; geri çevrilen: yalnızca süre.
    app.page_shown = back(40);
    key(&mut app, true);
    app.page_shown = back(20);
    key(&mut app, false);
    // 10 dakikadan uzun bekleme ara sayılır.
    app.page_shown = back(1200);
    key(&mut app, true);
    // Menü açılınca o ana kadarki süre eklenir.
    app.page_shown = back(30);
    middle(&mut app);
    assert!(app.page_shown.is_none());
    let id = session(&app).id.clone();
    let book = app.stats.books[&id];
    assert_eq!((book.secs / 10, book.pages), (9, 1), "40 + 20 + 30 sn");
    app.shutdown();

    let mut app = restart(&data, &root);
    assert_eq!(app.stats.books[&id].pages, 1, "kalıcı");
    tap(&mut app, super::ui::back_button(W).center());
    assert!(matches!(app.view, View::Backup));
    let at = app.stats_button().center();
    tap(&mut app, at);
    assert!(matches!(app.view, View::Stats));
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    tap(&mut app, super::ui::back_button(W).center());
    assert!(matches!(app.view, View::Backup));
}

#[test]
fn bookcase_groups_books_by_folder_and_opens_spines() {
    let (mut app, _, root) = setup(&[("a.epub", epub3_sample())]);
    std::fs::create_dir_all(root.join("Romanlar")).unwrap();
    std::fs::write(root.join("Romanlar/b.epub"), epub2_sample()).unwrap();
    std::fs::write(root.join("Romanlar/c.epub"), long_book()).unwrap();
    app.rescan();
    let (_, _, view, _) = app.tool_rects();
    tap(&mut app, view.center());
    tap(&mut app, view.center());
    assert_eq!(
        app.settings().library_view,
        super::shelf::LibraryView::Shelves
    );
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    let books = app.visible_books();
    assert_eq!(app.library_pages(&books), 1, "iki raf, iki sıra");

    // Ana klasör rafı önce; ikinci rafın ilk sırtı Romanlar'ın ilk kitabı.
    let spine = |row: i32, slot: i32| {
        let x = (super::ui::MARGIN + slot * 78 + 36) as u32;
        let y = (LIBRARY.top + row * 380 + 56 + 200) as u32;
        (x, y)
    };
    let first = app
        .library_index_at(&books, 0, spine(0, 0).0, spine(0, 0).1)
        .unwrap();
    assert_eq!(app.library[books[first]].display_title(), "Çalıkuşu");
    assert_eq!(
        app.library_index_at(&books, 0, spine(0, 1).0, spine(0, 1).1),
        None
    );
    let (x, y) = spine(1, 1);
    let i = app.library_index_at(&books, 0, x, y).unwrap();
    assert!(
        app.library[books[i]]
            .path
            .starts_with(root.join("Romanlar"))
    );
    app.handle(Event::Hold { x, y });
    assert!(matches!(app.view, View::BookActions { .. }));
    tap(&mut app, (10, 10));
    tap(&mut app, (x, y));
    assert!(app.session.is_some(), "sırta dokununca kitap açılır");
}

#[test]
fn series_books_stand_together_in_series_order_on_shelves() {
    let s = |name: &str, i: &str| {
        format!(
            r#"<meta name="calibre:series" content="{name}"/><meta name="calibre:series_index" content="{i}"/>"#
        )
    };
    let (mut app, _, _) = setup(&[
        ("b.epub", epub_with_meta("Birinci", &s("Vakıf", "2"))),
        ("a.epub", epub_with_meta("Ağaç", "")),
        ("z.epub", epub_with_meta("Zeta", &s("Vakıf", "1"))),
        ("y.epub", epub_with_meta("Yarım", &s("Vakıf", "1.5"))),
    ]);
    app.settings.library_sort = super::shelf::Sort::Title;
    app.settings.library_view = super::shelf::LibraryView::Shelves;
    let books = app.visible_books();
    let on_shelf: Vec<String> = (0..4)
        .map(|slot| {
            let x = (super::ui::MARGIN + slot * 78 + 36) as u32;
            let y = (LIBRARY.top + 56 + 200) as u32;
            let i = app.library_index_at(&books, 0, x, y).unwrap();
            app.library[books[i]].display_title()
        })
        .collect();
    assert_eq!(on_shelf, ["Ağaç", "Zeta", "Yarım", "Birinci"]);
    let label = |t: &str| {
        app.library
            .iter()
            .find(|e| e.display_title() == t)
            .unwrap()
            .series_label()
    };
    assert_eq!(label("Zeta").as_deref(), Some("Vakıf #1"));
    assert_eq!(label("Yarım").as_deref(), Some("Vakıf #1,5"));
    assert_eq!(label("Ağaç"), None);
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
}

/// Açık formun alanlarını doğrudan doldurur (klavyeyle yazmak testte yavaş).
fn fill_form(app: &mut App, values: &[&str]) {
    let View::Form(f) = &mut app.view else {
        panic!("form açık değil");
    };
    for (field, v) in f.fields.iter_mut().zip(values) {
        field.value = v.to_string();
    }
}

#[test]
fn book_info_is_edited_without_touching_the_file() {
    let (mut app, data, root) = setup(&[("b.epub", epub2_sample())]);
    let before = std::fs::read(root.join("b.epub")).unwrap();
    let (x, y) = LIBRARY.row_rect(W, 0).center();
    app.handle(Event::Hold { x, y });
    let at = app.status_rect(super::shelf::INFO_ROW).center();
    tap(&mut app, at);
    assert_eq!(form(&app).fields[0].value, "Kuyucaklı Yusuf");

    fill_form(
        &mut app,
        &["Kuyucaklı Yusuf", "Sabahattin Ali", "Romanlar", "iki"],
    );
    form_button(&mut app, form::Button::Submit);
    assert!(form(&app).error.is_some(), "sıra sayı değil");
    fill_form(&mut app, &["", "Sabahattin Ali, X", "Romanlar", "2,5"]);
    form_button(&mut app, form::Button::Submit);
    assert!(matches!(app.view, View::Library { .. }));
    let e = &app.library[0];
    assert_eq!(
        e.display_title(),
        "Kuyucaklı Yusuf",
        "boş başlık: kitabın kendi başlığı"
    );
    assert_eq!(e.authors, ["Sabahattin Ali", "X"]);
    assert_eq!(e.series_label().as_deref(), Some("Romanlar #2,5"));
    assert_eq!(std::fs::read(root.join("b.epub")).unwrap(), before);

    let app = restart(&data, &root);
    assert_eq!(app.library[0].authors, ["Sabahattin Ali", "X"], "kalıcı");
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
}

#[test]
fn right_to_left_books_turn_with_left_taps_and_swap_spread_pages() {
    let black = png(100, 150, 0);
    let white = png(100, 150, 255);
    let cbz = zip_of_bytes(&[("1.png", &black), ("2.png", &white), ("3.png", &black)]);
    let (mut app, _, _) = setup(&[("manga.cbz", cbz)]);
    tap_row(&mut app, &LIBRARY, 0);
    assert!(matches!(app.view, View::Reading));
    let mut next = app.settings.clone();
    next.rtl = true;
    app.apply_settings(next);
    // Sol kenar ileri, sağ kenar geri.
    tap(&mut app, (40, LIBRA2_HEIGHT / 2));
    assert_eq!(session(&app).spine, 1);
    tap(&mut app, (W - 40, LIBRA2_HEIGHT / 2));
    assert_eq!(session(&app).spine, 0);
    // Yatayda iki sayfa: ilk (siyah) sayfa sağ yarıda.
    app.handle(Event::Tilt { landscape: true });
    assert!(session(&app).right.is_some());
    let mut c = Canvas::new(app.width, app.height);
    app.draw(&mut c);
    let px = |x: u32| c.pixels()[((app.height / 2) * app.width + x) as usize];
    assert!(px(app.width / 4) > 200, "sol yarıda beyaz ikinci sayfa");
    assert!(px(3 * app.width / 4) < 50, "sağ yarıda siyah ilk sayfa");
}

#[test]
fn saved_words_are_reviewed_as_cards_and_rescheduled() {
    let (mut app, data, root) = setup(&[]);
    app.toggle_saved("ağaç", "ağaçlar");
    app.toggle_saved("çay", "çay");
    app.view = View::Words { page: 0 };
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    let at = app.review_button().center();
    tap(&mut app, at);
    let card = |app: &App| match &app.view {
        View::Review { card, answer, done } => (*card, answer.is_some(), *done),
        _ => panic!("tekrar ekranı değil"),
    };
    assert_eq!(card(&app), (Some(0), false, 0), "en eski kart önce");
    app.draw(&mut c);
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2));
    assert_eq!(card(&app), (Some(0), true, 0), "kart çevrildi");
    app.draw(&mut c);
    // Bilmedim: kart bugün yeniden sorulur, önce öbürü gelir.
    let again = app.grade_rect(0).center();
    tap(&mut app, again);
    assert_eq!(card(&app), (Some(1), false, 0));
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2));
    let good = app.grade_rect(2).center();
    tap(&mut app, good);
    assert_eq!(card(&app), (Some(0), false, 1));
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2));
    tap(&mut app, good);
    assert_eq!(card(&app), (None, false, 2), "bugünlük bitti");
    app.draw(&mut c);
    assert!(app.due_words().is_empty());

    let app = restart(&data, &root);
    assert!(app.due_words().is_empty(), "kalıcı");
    assert_eq!(app.words[1].review.interval, 1);
}

/// Yükleme sunucusuna ham HTTP isteği; (durum kodu, gövde).
fn http(addr: &str, method: &str, target: &str, body: &[u8]) -> (u16, String) {
    use std::io::{Read, Write};
    let host = addr.trim_start_matches("http://");
    let mut s = std::net::TcpStream::connect(host).unwrap();
    let head = format!(
        "{method} {target} HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    s.write_all(head.as_bytes()).unwrap();
    s.write_all(body).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let code = out[9..12].parse().unwrap();
    let body = out
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();
    (code, body)
}

#[test]
fn books_are_uploaded_over_wifi_with_the_screen_code() {
    let (mut app, _, root) = setup(&[]);
    let ip = std::net::IpAddr::from([127, 0, 0, 1]);
    app.upload = super::upload::Server::start(root.join("İndirilenler"), Some(ip));
    app.view = View::Upload;
    let (addr, code) = {
        let s = app.upload.as_ref().unwrap();
        (s.address.clone(), s.code.clone())
    };
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    let (status, page) = http(&addr, "GET", "/", b"");
    assert_eq!(status, 200);
    assert!(page.contains("/yukle"));

    let book = epub3_sample();
    let put = |name: &str, code: &str, body: &[u8]| {
        http(&addr, "PUT", &format!("/yukle?ad={name}&kod={code}"), body).0
    };
    assert_eq!(put("a.epub", "000000x", &book), 403, "yanlış kod");
    assert_eq!(put("G%C3%BCzel%20Kitap.epub", &code, &book), 200);
    assert_eq!(put("G%C3%BCzel%20Kitap.epub", &code, &book), 200);
    assert_eq!(put("betik.sh", &code, b"rm -rf /"), 400);
    assert_eq!(put("..%2F..%2Fdisari.epub", &code, &book), 200);
    let dir = root.join("İndirilenler");
    assert!(dir.join("Güzel Kitap.epub").exists());
    assert!(dir.join("Güzel Kitap (2).epub").exists());
    assert!(dir.join("disari.epub").exists(), "klasör dışına çıkamaz");
    assert!(!dir.join("betik.sh").exists());

    // Haberler Tick ile gelir; kütüphane yenilenir.
    for _ in 0..50 {
        app.handle(Event::Tick);
        if app.upload.as_ref().unwrap().saved.len() == 3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(app.upload.as_ref().unwrap().saved.len(), 3);
    assert_eq!(app.library.len(), 3);
    app.draw(&mut c);

    // Çok yanlış kod: sunucu kapanır.
    for _ in 0..9 {
        assert_eq!(put("a.epub", "1", &book), 403);
    }
    for _ in 0..50 {
        app.handle(Event::Tick);
        if app.upload.as_ref().unwrap().locked {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(app.upload.as_ref().unwrap().locked);
    tap(&mut app, super::ui::back_button(W).center());
    assert!(app.upload.is_none(), "ekrandan çıkınca kapanır");
}

/// Sahte DeepL: doğru anahtarla gelen isteğe çeviri, yanlışa 403 döner.
fn deepl_server() -> String {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let (mut auth, mut len) = (String::new(), 0usize);
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
                let lower = line.to_lowercase();
                if lower.starts_with("authorization:") {
                    auth = line[14..].trim().to_string();
                }
                if let Some(v) = lower.strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            let body = String::from_utf8(body).unwrap();
            let (status, answer) = if auth == "DeepL-Auth-Key dogru:fx" && body.contains("\"TR\"") {
                (
                    "200 OK",
                    r#"{"translations":[{"detected_source_language":"EN","text":"Uzun bir paragraf"}]}"#,
                )
            } else {
                ("403 Forbidden", r#"{"message":"Authorization failed"}"#)
            };
            let mut s = stream;
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
                answer.len()
            );
        }
    });
    url
}

#[test]
fn selected_text_is_translated_with_the_users_deepl_key() {
    let (mut app, data, root) = setup(&[("uzun.epub", long_book())]);
    app.endpoints.deepl = Some(deepl_server());
    // Anahtar yokken çubukta "Çevir" yok (5 düğme).
    tap_row(&mut app, &LIBRARY, 0);
    tap_row(&mut app, &CONTENTS, 0);
    let (x, y) = word_point(&app, "paragraf");
    app.handle(Event::Hold { x, y });
    app.handle(Event::HoldEnd { x, y });
    assert!(!app.can_translate());
    tap(&mut app, (W / 2, LIBRA2_HEIGHT - 5));

    // Anahtar Araçlar'dan girilir; veri klasöründe, yedekte değil.
    app.view = View::Services;
    let at = app.deepl_button().center();
    tap(&mut app, at);
    fill_form(&mut app, &["yanlis:fx"]);
    form_button(&mut app, form::Button::Submit);
    assert!(data.join("deepl.json").exists());
    app.view = View::Reading;
    select_and(&mut app, "paragraf", 4); // Çevir
    settle(&mut app);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::TranslateFailed(_),
            ..
        }
    ));

    app.save_deepl_key("dogru:fx");
    app.view = View::Reading;
    select_and(&mut app, "paragraf", 4);
    assert!(matches!(app.view, View::Searching));
    settle(&mut app);
    assert!(matches!(app.view, View::Lookup { .. }), "çeviri gösterildi");
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);

    app.save_deepl_key("");
    assert!(!data.join("deepl.json").exists());
    let _ = root;
}

/// Sahte WebDAV: Basic yetki ister (ad: ali, şifre: gizli); /dav/ altında bir
/// klasör ve bir kitap, PUT ile gelen yedeği saklar.
fn webdav_server() -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::{Arc, Mutex};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let log = Arc::new(Mutex::new(Vec::new()));
    let seen = log.clone();
    std::thread::spawn(move || {
        let mut stored: Option<Vec<u8>> = None;
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let (mut auth, mut len) = (String::new(), 0usize);
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
                let lower = line.to_lowercase();
                if lower.starts_with("authorization:") {
                    auth = line[14..].trim().to_string();
                }
                if let Some(v) = lower.strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            let mut parts = first.split_whitespace();
            let (method, path) = (
                parts.next().unwrap().to_string(),
                parts.next().unwrap().to_string(),
            );
            seen.lock()
                .unwrap()
                .push(format!("{method} {path} {}", !auth.is_empty()));
            // "ali:gizli" base64.
            let (status, extra, answer): (&str, &str, Vec<u8>) = if auth != "Basic YWxpOmdpemxp" {
                (
                    "401 Unauthorized",
                    "WWW-Authenticate: Basic realm=\"dav\"\r\n",
                    Vec::new(),
                )
            } else {
                match (method.as_str(), path.as_str()) {
                    ("PROPFIND", "/dav/") => ("207 Multi-Status", "", br#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:">
<d:response><d:href>/dav/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response>
<d:response><d:href>/dav/Romanlar/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response>
<d:response><d:href>/dav/kitap.epub</d:href><d:propstat><d:prop><d:resourcetype/></d:prop></d:propstat></d:response>
</d:multistatus>"#.to_vec()),
                    ("PROPFIND", "/dav/Romanlar/") => ("207 Multi-Status", "", br#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>/dav/Romanlar/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response></d:multistatus>"#.to_vec()),
                    ("GET", "/dav/kitap.epub") => ("200 OK", "", epub2_sample()),
                    ("PUT", "/dav/Legentia%20yedek.json") => {
                        stored = Some(body);
                        ("201 Created", "", Vec::new())
                    }
                    ("GET", "/dav/Legentia%20yedek.json") => match &stored {
                        Some(b) => ("200 OK", "", b.clone()),
                        None => ("404 Not Found", "", Vec::new()),
                    },
                    _ => ("404 Not Found", "", Vec::new()),
                }
            };
            let mut s = stream;
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
                answer.len()
            );
            let _ = s.write_all(&answer);
        }
    });
    (url, log)
}

#[test]
fn webdav_sends_and_merges_backups_and_downloads_books() {
    let (url, log) = webdav_server();
    let (mut app, data, root) = setup(&[("a.epub", epub3_sample())]);
    app.view = View::Services;
    let at = app.webdav_button().center();
    tap(&mut app, at);
    assert!(matches!(app.view, View::Form(_)), "hesap yokken önce form");
    fill_form(&mut app, &["ftp://x", "ali", "gizli"]);
    form_button(&mut app, form::Button::Submit);
    assert!(form(&app).error.is_some(), "geçersiz adres");
    fill_form(&mut app, &[&format!("{url}/dav"), "ali", "gizli"]);
    form_button(&mut app, form::Button::Submit);
    assert!(matches!(app.view, View::WebDav));
    assert!(data.join("webdav.json").exists());

    // Yedek yokken alma: anlaşılır mesaj.
    let [push, pull, browse, _] = app.dav_buttons();
    tap(&mut app, pull.center());
    settle(&mut app);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::DavNoBackup,
            ..
        }
    ));
    // Gönder, sonra yerelde kelime silinmiş olsa da al: birleşir.
    app.toggle_saved("ağaç", "ağaç");
    app.view = View::WebDav;
    tap(&mut app, push.center());
    settle(&mut app);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::DavPushed,
            ..
        }
    ));
    app.toggle_saved("ağaç", "ağaç");
    assert!(app.words.is_empty());
    app.view = View::WebDav;
    tap(&mut app, pull.center());
    settle(&mut app);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::RestoreDone { .. },
            ..
        }
    ));
    assert_eq!(app.words.len(), 1, "yedekteki kelime geri geldi");

    // Gezinme: klasör önce, kitap sonra; kitap indirilir.
    app.view = View::WebDav;
    tap(&mut app, browse.center());
    settle(&mut app);
    assert!(matches!(app.view, View::DavBrowse { page: 0 }));
    let names: Vec<String> = app
        .dav
        .dirs
        .last()
        .unwrap()
        .1
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(names, ["Romanlar", "kitap.epub"]);
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    tap_row(&mut app, &super::webdav::DAV_LIST, 1);
    settle(&mut app);
    assert!(root.join("İndirilenler/kitap.epub").exists());
    tap_row(&mut app, &super::webdav::DAV_LIST, 0);
    settle(&mut app);
    assert_eq!(app.dav.dirs.len(), 2, "alt klasöre girildi");
    app.draw(&mut c);
    tap(&mut app, super::ui::back_button(W).center());
    assert_eq!(app.dav.dirs.len(), 1);

    // Şifre ilk istekte gitmez, yalnızca 401'den sonra.
    let log = log.lock().unwrap();
    assert!(log.iter().any(|l| l.ends_with("false")) && log.iter().any(|l| l.ends_with("true")));
    for pair in log.chunks(2) {
        assert!(pair[0].ends_with("false"), "{log:?}");
    }
}

#[test]
fn news_feeds_are_added_updated_read_and_kept_offline() {
    let url = crate::net::testserver::serve(|path, _| match path {
        "/rss" => (
            200,
            r#"<rss version="2.0"><channel><title>Gazete</title>
<item><title>Birinci haber</title><guid>h1</guid><pubDate>Thu, 29 Oct 2026</pubDate>
<description>&lt;p&gt;Ağaçların &lt;b&gt;gölgesinde&lt;/b&gt; çay.&lt;/p&gt;</description></item>
<item><title>İkinci haber</title><guid>h2</guid><description>Kısa.</description></item>
</channel></rss>"#
                .as_bytes()
                .to_vec(),
        ),
        _ => (200, b"<html><body>sayfa</body></html>".to_vec()),
    });
    let (mut app, data, root) = setup(&[]);
    app.view = View::Backup;
    let at = app.news_button().center();
    tap(&mut app, at);
    assert!(matches!(app.view, View::NewsFeeds { page: 0 }));
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    // Besleme olmayan adres: güncellemede hata işareti.
    for u in [format!("{url}/rss"), format!("{url}/sayfa")] {
        tap(&mut app, news_button("add"));
        fill_form(&mut app, &[&u]);
        form_button(&mut app, form::Button::Submit);
    }
    assert_eq!(app.news.news.feeds.len(), 2);
    tap(&mut app, news_button("update"));
    settle(&mut app);
    let feeds = &app.news.news.feeds;
    assert_eq!(feeds[0].title, "Gazete");
    assert_eq!(feeds[0].items.len(), 2);
    assert_eq!(app.news.failed, [format!("{url}/sayfa")]);
    app.draw(&mut c);

    // Yazı açılır, okundu işaretlenir; ortaya dokununca listeye dönülür.
    tap_row(&mut app, &super::news::NEWS_LIST, 0);
    assert!(matches!(app.view, View::NewsItems { feed: 0, page: 0 }));
    app.draw(&mut c);
    tap_row(&mut app, &super::news::NEWS_LIST, 0);
    assert!(matches!(
        app.view,
        View::NewsArticle {
            feed: 0,
            item: 0,
            page: 0
        }
    ));
    assert!(!app.news.article[0].lines.is_empty(), "yazı sayfalandı");
    app.draw(&mut c);
    assert!(app.news.news.read.contains("h1"));
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2));
    assert!(matches!(app.view, View::NewsItems { .. }));

    // Uzun basınca düzenle; sil.
    tap(&mut app, super::ui::back_button(W).center());
    let (x, y) = super::news::NEWS_LIST.row_rect(W, 1).center();
    app.handle(Event::Hold { x, y });
    form_button(&mut app, form::Button::Delete);
    assert_eq!(app.news.news.feeds.len(), 1);

    // Çevrimdışı: yazılar ve okundu bilgisi kayıtta.
    let app = restart(&data, &root);
    assert_eq!(app.news.news.feeds[0].items.len(), 2);
    assert!(app.news.news.read.contains("h1"));
}

/// Haber ekranının başlık düğmeleri.
fn news_button(which: &str) -> (u32, u32) {
    let add = super::ui::header_button(W, 170);
    match which {
        "add" => add.center(),
        _ => super::ui::left_of(add, 220).center(),
    }
}

#[test]
fn updates_are_checked_downloaded_and_verified_on_the_device_only() {
    use sha2::{Digest, Sha256};
    use std::sync::atomic::{AtomicBool, Ordering};
    let program = b"yeni program".to_vec();
    let sum: String = Sha256::digest(&program)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let good = std::sync::Arc::new(AtomicBool::new(false));
    let flag = good.clone();
    let url = crate::net::testserver::serve(move |path, headers| {
        let host = headers
            .lines()
            .find_map(|l| {
                l.strip_prefix("host: ")
                    .or_else(|| l.strip_prefix("Host: "))
            })
            .unwrap_or_default()
            .trim()
            .to_string();
        match path {
            "/releases" => (
                200,
                format!(
                    r#"{{"tag_name":"v99.0.0","body":"Yenilikler\nİkinci satır","assets":[
                    {{"name":"legentia-kobo","browser_download_url":"http://{host}/b"}},
                    {{"name":"legentia-kobo.sha256","browser_download_url":"http://{host}/s"}}]}}"#
                )
                .into_bytes(),
            ),
            "/b" => (200, b"yeni program".to_vec()),
            "/s" if flag.load(Ordering::SeqCst) => {
                (200, format!("{sum}  legentia-kobo\n").into_bytes())
            }
            "/s" => (
                200,
                format!("{}  legentia-kobo\n", "0".repeat(64)).into_bytes(),
            ),
            _ => (404, Vec::new()),
        }
    });
    let (mut app, data, _) = setup(&[]);
    app.endpoints.releases = format!("{url}/releases");

    // Simülatörde (güncelleme klasörü yok) yalnızca bilgi verilir.
    app.view = View::Backup;
    let at = app.update_button().center();
    tap(&mut app, at);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::UpdateDeviceOnly,
            ..
        }
    ));

    let dir = data.join("program");
    std::fs::create_dir_all(&dir).unwrap();
    app.set_update_dir(dir.clone());
    app.view = View::Backup;
    tap(&mut app, at);
    settle(&mut app);
    assert!(
        matches!(app.update, Some(super::update::Step::Found(_))),
        "{:?}",
        match &app.view {
            View::Message { msg, .. } => Some(msg.clone()),
            _ => None,
        }
    );
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);

    // Özet uymazsa dosya silinir.
    let install = app.install_button().center();
    tap(&mut app, install);
    settle(&mut app);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::UpdateBadChecksum,
            ..
        }
    ));
    assert!(!dir.join("legentia-kobo.yeni").exists());

    good.store(true, Ordering::SeqCst);
    app.view = View::Backup;
    tap(&mut app, at);
    settle(&mut app);
    tap(&mut app, install);
    settle(&mut app);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::UpdateReady,
            ..
        }
    ));
    assert_eq!(
        std::fs::read(dir.join("legentia-kobo.yeni")).unwrap(),
        program
    );
}

/// Sahte Wallabag: iki okunmamış yazı; arşivlenen listeden çıkar.
fn wallabag_server() -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::{Arc, Mutex};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let log = Arc::new(Mutex::new(Vec::new()));
    let seen = log.clone();
    std::thread::spawn(move || {
        let mut unread: Vec<u64> = vec![1, 2];
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let (mut auth, mut len) = (String::new(), 0usize);
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
                let lower = line.to_lowercase();
                if lower.starts_with("authorization:") {
                    auth = line[14..].trim().to_string();
                }
                if let Some(v) = lower.strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            let body = String::from_utf8(body).unwrap();
            let mut parts = first.split_whitespace();
            let (method, path) = (
                parts.next().unwrap().to_string(),
                parts.next().unwrap().to_string(),
            );
            seen.lock().unwrap().push(format!("{method} {path} {body}"));
            let bearer = auth == "Bearer a1";
            let (status, answer): (&str, Vec<u8>) =
                match (method.as_str(), path.split('?').next().unwrap()) {
                    ("POST", "/oauth/v2/token")
                        if body.contains("client_id=c1")
                            && body.contains("client_secret=s1")
                            && (body
                                .contains("grant_type=password&username=ali&password=gizli")
                                || body.contains("grant_type=refresh_token&refresh_token=r1")) =>
                    {
                        (
                            "200 OK",
                            br#"{"access_token":"a1","refresh_token":"r1","expires_in":3600}"#
                                .to_vec(),
                        )
                    }
                    ("POST", _) => ("400 Bad Request", b"{}".to_vec()),
                    _ if !bearer => ("401 Unauthorized", Vec::new()),
                    ("GET", "/api/entries.json") => {
                        let items: Vec<String> = unread
                            .iter()
                            .map(|id| format!(r#"{{"id":{id},"title":"Yazı {id}"}}"#))
                            .collect();
                        (
                            "200 OK",
                            format!(
                                r#"{{"page":1,"pages":1,"_embedded":{{"items":[{}]}}}}"#,
                                items.join(",")
                            )
                            .into_bytes(),
                        )
                    }
                    ("GET", p) if p.ends_with("/export.epub") => {
                        ("200 OK", epub_with_meta("Yazı", ""))
                    }
                    ("PATCH", p) if body == "archive=1" => {
                        let id: u64 = p
                            .trim_start_matches("/api/entries/")
                            .trim_end_matches(".json")
                            .parse()
                            .unwrap();
                        unread.retain(|u| *u != id);
                        ("200 OK", b"{}".to_vec())
                    }
                    _ => ("404 Not Found", Vec::new()),
                };
            let mut s = stream;
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                answer.len()
            );
            let _ = s.write_all(&answer);
        }
    });
    (url, log)
}

#[test]
fn wallabag_articles_arrive_as_epubs_and_finished_ones_are_archived() {
    let (url, log) = wallabag_server();
    let (mut app, data, root) = setup(&[]);
    app.view = View::Services;
    let at = app.wallabag_button().center();
    tap(&mut app, at);
    fill_form(&mut app, &[&format!("{url}/"), "c1", "s1", "ali", "yanlis"]);
    form_button(&mut app, form::Button::Submit);
    settle(&mut app);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::WallabagLogin,
            ..
        }
    ));

    app.view = View::Services;
    tap(&mut app, at);
    fill_form(&mut app, &[&format!("{url}/"), "c1", "s1", "ali", "gizli"]);
    form_button(&mut app, form::Button::Submit);
    assert!(matches!(app.view, View::Working));
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    settle(&mut app);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::WallabagSynced {
                added: 2,
                archived: 0,
                removed: 0
            },
            ..
        }
    ));
    let dir = root.join("Wallabag");
    assert!(dir.join("Yazı 1 [1].epub").exists() && dir.join("Yazı 2 [2].epub").exists());
    assert_eq!(app.library.len(), 2);
    let saved = std::fs::read_to_string(data.join("wallabag.json")).unwrap();
    assert!(!saved.contains("gizli"), "şifre saklanmaz");

    // Biri bitti: eşitlemede arşivlenir ve cihazdan silinir.
    let i = app
        .library
        .iter()
        .position(|e| e.path.ends_with("Yazı 1 [1].epub"))
        .unwrap();
    app.set_status(i, super::shelf::ReadStatus::Finished);
    app.view = View::Services;
    tap(&mut app, at);
    settle(&mut app);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::WallabagSynced {
                added: 0,
                archived: 1,
                removed: 0
            },
            ..
        }
    ));
    assert!(!dir.join("Yazı 1 [1].epub").exists());
    assert!(dir.join("Yazı 2 [2].epub").exists());
    assert!(
        log.lock()
            .unwrap()
            .iter()
            .any(|l| l.starts_with("PATCH /api/entries/1.json"))
    );
    app.view = View::Services;
    app.draw(&mut c);
}

/// Sahte Dropbox (yetki, API ve içerik sunucusu tek yerde).
fn dropbox_server() -> String {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let base = url.clone();
    std::thread::spawn(move || {
        let mut stored: Option<Vec<u8>> = None;
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let (mut auth, mut arg, mut len) = (String::new(), String::new(), 0usize);
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
                let lower = line.to_lowercase();
                if lower.starts_with("authorization:") {
                    auth = line[14..].trim().to_string();
                }
                if lower.starts_with("dropbox-api-arg:") {
                    arg = line[16..].trim().to_string();
                }
                if let Some(v) = lower.strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            let text = String::from_utf8_lossy(&body).into_owned();
            let path = first.split_whitespace().nth(1).unwrap().to_string();
            let ok_auth = auth == "Bearer t";
            let (status, answer): (&str, Vec<u8>) = match path.as_str() {
                "/oauth2/token" if text.contains("grant_type=authorization_code") && text.contains("code=DBXKOD")
                    && text.contains("code_verifier=") && text.contains("client_id=0gzqlf7pa2k80lv") =>
                    ("200 OK", br#"{"access_token":"t","refresh_token":"r"}"#.to_vec()),
                "/oauth2/token" if text.contains("grant_type=refresh_token&refresh_token=r") =>
                    ("200 OK", br#"{"access_token":"t"}"#.to_vec()),
                "/oauth2/token" => ("400 Bad Request", br#"{"error":"invalid_grant"}"#.to_vec()),
                _ if !ok_auth && !path.starts_with("/dosya/") => ("401 Unauthorized", Vec::new()),
                "/2/files/list_folder" => ("200 OK", br#"{"entries":[
                    {".tag":"file","name":"kitap.epub","path_display":"/kitap.epub"},
                    {".tag":"file","name":"resim.jpg","path_display":"/resim.jpg"},
                    {".tag":"folder","name":"Romanlar","path_display":"/Romanlar"}],"cursor":"c","has_more":false}"#.to_vec()),
                "/2/files/upload" if arg.contains("Legentia yedek.json") && arg.contains("overwrite") => {
                    stored = Some(body);
                    ("200 OK", b"{}".to_vec())
                }
                "/2/files/download" => match &stored {
                    Some(b) => ("200 OK", b.clone()),
                    None => ("409 Conflict", br#"{"error_summary":"path/not_found/"}"#.to_vec()),
                },
                "/2/files/get_temporary_link" => ("200 OK", format!(r#"{{"link":"{base}/dosya/kitap.epub"}}"#).into_bytes()),
                "/dosya/kitap.epub" => ("200 OK", epub2_sample()),
                _ => ("404 Not Found", Vec::new()),
            };
            let mut s = stream;
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                answer.len()
            );
            let _ = s.write_all(&answer);
        }
    });
    url
}

#[test]
fn dropbox_connects_through_the_phone_page_and_syncs() {
    let url = dropbox_server();
    let (mut app, data, root) = setup(&[]);
    app.dropbox.hosts = super::dropbox::Hosts {
        auth: url.clone(),
        api: url.clone(),
        content: url.clone(),
    };
    app.start_dropbox_connect(Some(std::net::IpAddr::from([127, 0, 0, 1])));
    assert!(matches!(app.view, View::DropboxConnect));
    let mut c = Canvas::new(W, LIBRA2_HEIGHT);
    app.draw(&mut c);
    let (addr, code) = {
        let c = app.dropbox.connect.as_ref().unwrap();
        (c.address.clone(), c.code.clone())
    };
    let (_, page) = http(&addr, "GET", "/?kod=000000x", b"");
    assert!(
        !page.contains("oauth2/authorize"),
        "yanlış kodla bağlantı verilmez"
    );
    let (_, page) = http(&addr, "GET", &format!("/?kod={code}"), b"");
    assert!(page.contains("oauth2/authorize?client_id=0gzqlf7pa2k80lv"));
    let (_, page) = http(&addr, "GET", &format!("/?kod={code}&dbx=DBXKOD"), b"");
    assert!(page.contains("cihaza") || page.contains("device"));
    settle(&mut app);
    assert!(matches!(app.view, View::Dropbox), "bağlandı");
    assert!(app.dropbox.connect.is_none(), "sayfa kapandı");
    let saved = std::fs::read_to_string(data.join("dropbox.json")).unwrap();
    assert!(saved.contains("\"r\""));

    let [push, pull, browse, logout] = app.dropbox_buttons();
    tap(&mut app, pull.center());
    settle(&mut app);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::DavNoBackup,
            ..
        }
    ));
    app.view = View::Dropbox;
    tap(&mut app, push.center());
    settle(&mut app);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::DropboxPushed,
            ..
        }
    ));
    app.view = View::Dropbox;
    tap(&mut app, pull.center());
    settle(&mut app);
    assert!(matches!(
        app.view,
        View::Message {
            msg: Msg::RestoreDone { .. },
            ..
        }
    ));

    app.view = View::Dropbox;
    tap(&mut app, browse.center());
    settle(&mut app);
    let names: Vec<String> = app
        .dropbox
        .dirs
        .last()
        .unwrap()
        .1
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(names, ["Romanlar", "kitap.epub"]);
    app.draw(&mut c);
    tap_row(&mut app, &super::dropbox::DBX_LIST, 1);
    settle(&mut app);
    assert!(root.join("İndirilenler/kitap.epub").exists());

    app.view = View::Dropbox;
    tap(&mut app, logout.center());
    assert!(app.dropbox.account.is_none());
    assert!(!data.join("dropbox.json").exists());
}

#[test]
fn page_keys_can_be_swapped_separately_in_landscape() {
    let (mut app, _, _) = reading_long_book();
    key(&mut app, true);
    key(&mut app, true);
    let at = session(&app).page;
    app.settings.swap_keys_landscape = true;
    // Dikeyde ayar etkisiz.
    key(&mut app, true);
    assert_eq!(session(&app).page, at + 1);
    app.handle(Event::Tilt { landscape: true });
    assert!(app.width > app.height);
    let at = session(&app).page;
    key(&mut app, true);
    assert!(session(&app).page < at, "yatayda ileri tuşu geri çevirir");
}
