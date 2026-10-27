//! Uygulama: ekranlar arası geçiş, olay işleme, kayıt ve çizim.
//!
//! Ekranlar: kütüphane → (kitap) içindekiler / okuma → menü → yer imleri,
//! yazı ayarları. Okurken ekranın ortasına dokunmak menüyü açar.

mod screens;
mod session;
mod settings;
mod ui;

use std::collections::HashMap;
use std::path::Path;

use crate::canvas::Canvas;
use crate::epub::Book;
use crate::event::{Event, Refresh};
use crate::i18n::Msg;
use crate::library::{self, Entry};
use crate::storage::Store;
use crate::text::Text;
pub use session::{BookState, Bookmark, Position};
use session::{Session, Target};
pub use settings::Settings;

/// Kaç kısmi yenilemeden sonra bir tam yenileme yapılacağı.
/// Geçici değer; gerçek cihazda gölgelenmeye bakılarak ayarlanacak.
const PARTIALS_PER_FULL: u32 = 6;
const SETTINGS_FILE: &str = "settings.json";
/// Konum en geç bu kadar sayfa çevirmede bir kaydedilir. Kayıt `fsync` içerdiği
/// için her çevirmede yazmak (masaüstünde ~10 ms) sayfa çevirmeyi yavaşlatır;
/// beklenmedik kapanmada en fazla bu kadar sayfa geri dönülür.
const SAVE_EVERY_TURNS: u32 = 10;

enum View {
    Library { page: usize },
    Message { msg: Msg, back: Back },
    Contents { page: usize },
    Reading,
    Menu,
    Bookmarks { page: usize },
    Settings { draft: Settings },
}

/// Mesaj ekranından dokununca dönülecek yer.
#[derive(Clone, Copy)]
enum Back {
    Library,
    Contents,
}

pub struct App {
    text: Text,
    width: u32,
    store: Store,
    settings: Settings,
    library: Vec<Entry>,
    /// Kitap kimliği → okunma yüzdesi (kütüphanede gösterilir).
    progress: HashMap<String, u32>,
    session: Option<Session>,
    view: View,
    since_full: u32,
    /// Son kayıttan beri çevrilen sayfa sayısı.
    unsaved_turns: u32,
}

impl App {
    /// Ayarları yükler, `library_root` altındaki kitapları tarar ve kütüphaneyle açılır.
    pub fn new(text: Text, width: u32, store: Store, library_root: &Path) -> Self {
        let settings = store.load(SETTINGS_FILE).unwrap_or_default();
        let library = library::scan(library_root, &store);
        let progress = library
            .iter()
            .filter_map(|e| {
                store
                    .load::<BookState>(&book_file(&e.id))
                    .map(|s| (e.id.clone(), s.percent))
            })
            .collect();
        Self {
            text,
            width,
            store,
            settings,
            library,
            progress,
            session: None,
            view: View::Library { page: 0 },
            since_full: 0,
            unsaved_turns: 0,
        }
    }

    /// İlk açılışta ekran her zaman tam yenilenir.
    pub fn start(&mut self) -> Refresh {
        self.refresh(true)
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Kitabı açar: kayıtlı konum varsa oradan, yoksa içindekilerden.
    pub fn open_book(&mut self, path: &Path) {
        let id = self
            .library
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.id.clone());
        let size = std::fs::metadata(path).map_or(0, |m| m.len());
        let book = match Book::open(path) {
            Ok(b) => b,
            Err(e) => {
                log::error!("cannot open {}: {e:?}", path.display());
                self.view = View::Message {
                    msg: e.message(),
                    back: Back::Library,
                };
                return;
            }
        };
        let id = id.unwrap_or_else(|| library::book_id(book.identifier.as_deref(), path, size));
        let state: BookState = self.store.load(&book_file(&id)).unwrap_or_default();
        let last = state.last;
        self.session = Some(Session::new(book, id, state));
        self.view = View::Contents { page: 0 };
        if let Some((spine, at)) = last {
            self.open_chapter(spine, Target::At(at));
        }
    }

    /// Bir olayı işler; ekran değişmediyse `None` döner (E Ink'e boşuna yenileme yok).
    pub fn handle(&mut self, event: Event) -> Option<Refresh> {
        match &self.view {
            View::Library { page } => self.handle_library(*page, event),
            View::Message { back, .. } => {
                let back = *back;
                matches!(event, Event::Tap { .. }).then(|| {
                    self.view = match back {
                        Back::Library => View::Library { page: 0 },
                        Back::Contents => View::Contents { page: 0 },
                    };
                    self.refresh(true)
                })
            }
            View::Contents { page } => self.handle_contents(*page, event),
            View::Reading => self.handle_reading(event),
            View::Menu => self.handle_menu(event),
            View::Bookmarks { page } => self.handle_bookmarks(*page, event),
            View::Settings { draft } => {
                let draft = draft.clone();
                self.handle_settings(draft, event)
            }
        }
    }

    fn refresh(&mut self, force_full: bool) -> Refresh {
        if force_full || self.since_full + 1 >= PARTIALS_PER_FULL {
            self.since_full = 0;
            Refresh::Full
        } else {
            self.since_full += 1;
            Refresh::Partial
        }
    }

    /// Bölümü açıp okuma ekranına geçer; okunamazsa hata gösterir.
    fn open_chapter(&mut self, spine: usize, target: Target) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        match session.load_chapter(&self.text, &self.settings, self.width, spine, target) {
            Ok(()) => {
                self.view = View::Reading;
                self.save_progress();
            }
            Err(e) => {
                log::error!("cannot read chapter {spine}: {e:?}");
                self.view = View::Message {
                    msg: e.message(),
                    back: Back::Contents,
                };
            }
        }
    }

    /// Uygulama kapanırken (ya da cihaz uykuya geçerken) çağrılmalı: bekleyen konumu kaydeder.
    pub fn shutdown(&mut self) {
        if self.unsaved_turns > 0 {
            self.save_progress();
        }
    }

    /// Sayfa çevrildi: konum her `SAVE_EVERY_TURNS` çevirmede bir kaydedilir.
    fn page_turned(&mut self) {
        self.unsaved_turns += 1;
        if self.unsaved_turns >= SAVE_EVERY_TURNS {
            self.save_progress();
        }
    }

    /// Okuma konumunu ve yer imlerini hemen kaydeder (dosya birkaç yüz bayt).
    fn save_progress(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        self.unsaved_turns = 0;
        session.record();
        self.store.save(&book_file(&session.id), &session.state);
        self.progress
            .insert(session.id.clone(), session.state.percent);
    }

    fn apply_settings(&mut self, settings: Settings) {
        if settings == self.settings {
            return;
        }
        self.settings = settings;
        self.store.save(SETTINGS_FILE, &self.settings);
        if let Some(session) = self.session.as_mut() {
            session.relayout(&self.text, &self.settings, self.width);
        }
        self.save_progress();
    }

    pub fn draw(&self, canvas: &mut Canvas) {
        canvas.fill(255);
        match &self.view {
            View::Library { page } => self.draw_library(canvas, *page),
            View::Message { msg, .. } => self.draw_message(canvas, msg),
            View::Contents { page } => self.draw_contents(canvas, *page),
            View::Reading => self.draw_reading(canvas),
            View::Menu => {
                self.draw_reading(canvas);
                self.draw_menu(canvas);
            }
            View::Bookmarks { page } => self.draw_bookmarks(canvas, *page),
            View::Settings { draft } => self.draw_settings(canvas, draft),
        }
        if self.settings.night {
            canvas.invert();
        }
    }
}

fn book_file(id: &str) -> String {
    format!("books/{id}.json")
}

#[cfg(test)]
mod tests;
