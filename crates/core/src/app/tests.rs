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
    app.settings.online_tdk = false;
    app.settings.online_wiktionary = false;
    app.endpoints = super::lookup::Endpoints {
        tdk: "http://127.0.0.1:1/gts".into(),
        wiktionary: "http://127.0.0.1:1/api.php".into(),
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

fn middle(app: &mut App) -> Option<Refresh> {
    tap(app, (W / 2, LIBRA2_HEIGHT / 2))
}

fn menu(app: &mut App, item: MenuItem) -> Option<Refresh> {
    middle(app);
    assert!(matches!(app.view, View::Menu));
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

    let app = restart(&data, &root);
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
    let i = s.add_annotation(start, end).unwrap();
    assert_eq!(s.state.annotations[i].quote, "bir paragraf: a");
    assert_eq!(s.placed, vec![(i, start, end)]);
    assert_eq!(s.add_annotation(end, start), None, "boş aralık");
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
    let at = app.action_rect(1).center(); // Vurguyu sil
    tap(&mut app, at);
    assert!(session(&app).state.annotations.is_empty());
    assert!(matches!(app.view, View::Reading));
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
            W,
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
    let i = s.add_annotation(at, end).unwrap();
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
    let i = s.add_annotation(start, end).unwrap();
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
    assert_eq!(app.settings().font_px, size);
    tap(&mut app, (W / 2, LIBRA2_HEIGHT / 2)); // mesajdan kütüphaneye
    tap_row(&mut app, &LIBRARY, 0);
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
    app.settings.online_tdk = true;
    app.settings.online_wiktionary = true;
    app.endpoints = super::lookup::Endpoints {
        tdk: format!("{url}/gts"),
        wiktionary: format!("{url}/api.php"),
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
