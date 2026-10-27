use std::path::{Path, PathBuf};

use super::screens::{BOOKMARKS, CONTENTS, LIBRARY, MENU, MenuItem};
use super::settings::{FIELDS, Field};
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
    (App::new(test_text(), W, store, &root), data, root)
}

fn restart(data: &Path, root: &Path) -> App {
    App::new(test_text(), W, Store::new(data), root)
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
    let index = MENU.iter().position(|m| *m == item).unwrap();
    tap(app, app.menu_rect(index).center())
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
    let size_row = FIELDS.iter().position(|f| *f == Field::Size).unwrap();
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
    menu(&mut app, MenuItem::Settings);
    let (_, plus) = app.settings_buttons(0);
    tap(&mut app, plus.center());
    let at = app.settings_cancel().center();
    tap(&mut app, at);
    assert_eq!(app.settings(), &Settings::default());
    assert!(matches!(app.view, View::Reading));
}

#[test]
fn night_mode_inverts_the_screen() {
    let (mut app, _, _) = reading_long_book();
    menu(&mut app, MenuItem::Settings);
    let night = FIELDS.iter().position(|f| *f == Field::Night).unwrap();
    let at = app.settings_buttons(night).1.center();
    tap(&mut app, at);
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
    tap(&mut app, (W / 2, LIBRA2_HEIGHT - 200));
    assert!(matches!(app.view, View::Reading));
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
        let index = MENU.iter().position(|m| *m == item).unwrap();
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
