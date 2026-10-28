//! Uygulama: ekranlar arası geçiş, olay işleme, kayıt ve çizim.
//!
//! Ekranlar: kütüphane → (kitap) içindekiler / okuma → menü → yer imleri,
//! yazı ayarları. Okurken ekranın ortasına dokunmak menüyü açar.
//! PDF'ler aynı ekranlarla okunur; sayfa görüntüsü çizilip önbellekte tutulur.

mod annotations;
mod backup;
mod export;
mod keyboard;
mod lookup;
mod note;
mod pdf_view;
mod screens;
mod select;
mod session;
mod settings;
mod ui;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::canvas::Canvas;
use crate::epub::Book;
use crate::event::{Event, Gestures, Refresh};
use crate::i18n::Msg;
use crate::library::{self, Entry, Format};
use crate::pdf::{GrayImage, PdfDoc, PdfEdit};
use crate::storage::Store;
use crate::text::Text;
pub use annotations::Annotation;
pub use session::{BookState, Bookmark, Position};
use session::{Doc, Session, Target};
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
    Library {
        page: usize,
    },
    Message {
        msg: Msg,
        back: Back,
    },
    Contents {
        page: usize,
    },
    Reading,
    Menu,
    Bookmarks {
        page: usize,
    },
    Annotations {
        page: usize,
    },
    /// Yedekleme ve geri yükleme (kütüphaneden).
    Backup,
    /// Sözlük sonucu (`lookup`), sayfası.
    Lookup {
        page: usize,
    },
    Settings {
        draft: Settings,
        page: usize,
    },
    /// Metin seçili; işlem çubuğu açık.
    Selected,
    /// Vurguya dokunuldu (`state.annotations` sırası); işlem çubuğu açık.
    Annotation {
        index: usize,
    },
    /// Vurgunun notu yazılıyor.
    Note {
        index: usize,
        draft: String,
        keyboard: keyboard::Keyboard,
    },
}

/// Mesaj ekranından dokununca dönülecek yer.
#[derive(Clone, Copy)]
enum Back {
    Library,
    Contents,
    Reading,
}

pub struct App {
    text: Text,
    width: u32,
    store: Store,
    /// Kütüphane klasörü; yedek dosyası buraya yazılır.
    root: PathBuf,
    settings: Settings,
    library: Vec<Entry>,
    /// Kitap kimliği → okunma yüzdesi (kütüphanede gösterilir).
    progress: HashMap<String, u32>,
    session: Option<Session>,
    view: View,
    since_full: u32,
    /// Son kayıttan beri çevrilen sayfa sayısı.
    unsaved_turns: u32,
    /// Son çizilen PDF ekranı; menü açılıp kapanınca yeniden çizilmez.
    pdf_frame: RefCell<Option<PdfFrame>>,
    gestures: Gestures,
    /// Okurken uzun basmayla başlayan seçim.
    selection: Option<select::Selection>,
    /// Sözlükler; ilk aramada açılır.
    dicts: Option<crate::dict::Dictionaries>,
    /// Gösterilen sözlük sonucu.
    lookup: Option<lookup::Lookup>,
}

/// Ekrana yerleştirilmiş PDF sayfa görüntüsü.
struct PdfFrame {
    page: usize,
    screen: pdf_view::Screen,
    image: GrayImage,
    x: i32,
    y: i32,
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
            root: library_root.to_path_buf(),
            settings,
            library,
            progress,
            session: None,
            view: View::Library { page: 0 },
            since_full: 0,
            unsaved_turns: 0,
            pdf_frame: RefCell::new(None),
            gestures: Gestures::default(),
            selection: None,
            dicts: None,
            lookup: None,
        }
    }

    /// İlk açılışta ekran her zaman tam yenilenir.
    pub fn start(&mut self) -> Refresh {
        self.refresh(true)
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Kitabı açar: kayıtlı konum varsa oradan, yoksa EPUB'da içindekilerden,
    /// PDF'te ilk sayfadan.
    pub fn open_book(&mut self, path: &Path) {
        let id = self
            .library
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.id.clone());
        let size = std::fs::metadata(path).map_or(0, |m| m.len());
        let opened = match Format::of(path).unwrap_or_default() {
            Format::Epub => Book::open(path)
                .map(|b| {
                    let id =
                        id.unwrap_or_else(|| library::book_id(b.identifier.as_deref(), path, size));
                    (Doc::Epub(b), id)
                })
                .map_err(|e| (format!("{e:?}"), e.message())),
            Format::Pdf => PdfDoc::open(path)
                .map(|d| {
                    (
                        Doc::Pdf(d),
                        id.unwrap_or_else(|| library::book_id(None, path, size)),
                    )
                })
                .map_err(|e| (format!("{e:?}"), e.message())),
        };
        let (doc, id) = match opened {
            Ok(v) => v,
            Err((detail, msg)) => {
                log::error!("cannot open {}: {detail}", path.display());
                self.view = View::Message {
                    msg,
                    back: Back::Library,
                };
                return;
            }
        };
        let is_pdf = matches!(doc, Doc::Pdf(_));
        let state: BookState = self.store.load(&book_file(&id)).unwrap_or_default();
        let last = state.last;
        let name = path
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut session = Session::new(doc, name, id.clone(), state);
        session.path = path.to_path_buf();
        if is_pdf {
            session.ocr_cache = Some(Store::new(&self.store.dir().join("ocr").join(&id)));
        }
        self.session = Some(session);
        self.pdf_frame.replace(None);
        self.view = View::Contents { page: 0 };
        match last {
            Some((spine, at)) => self.open_chapter(spine, Target::At(at)),
            None if is_pdf => self.open_chapter(0, Target::Start),
            None => {}
        }
    }

    /// Bir olayı işler; ekran değişmediyse `None` döner (E Ink'e boşuna yenileme yok).
    /// Ham dokunma birden çok harekete dönüşebilir; biri tam yenileme isterse tam yenilenir.
    pub fn handle(&mut self, event: Event) -> Option<Refresh> {
        if let Event::Touch { phase, x, y, ms } = event {
            return self
                .gestures
                .feed(phase, x, y, ms)
                .into_iter()
                .filter_map(|e| self.handle(e))
                .max_by_key(|r| *r == Refresh::Full);
        }
        match &self.view {
            View::Library { page } => self.handle_library(*page, event),
            View::Message { back, .. } => {
                let back = *back;
                matches!(event, Event::Tap { .. }).then(|| {
                    self.view = match back {
                        Back::Library => View::Library { page: 0 },
                        Back::Contents => View::Contents { page: 0 },
                        Back::Reading => View::Reading,
                    };
                    self.refresh(true)
                })
            }
            View::Contents { page } => self.handle_contents(*page, event),
            View::Reading => self.handle_reading(event),
            View::Menu => self.handle_menu(event),
            View::Bookmarks { page } => self.handle_bookmarks(*page, event),
            View::Annotations { page } => self.handle_annotations(*page, event),
            View::Backup => self.handle_backup(event),
            View::Lookup { page } => self.handle_lookup(*page, event),
            View::Settings { draft, page } => {
                let (draft, page) = (draft.clone(), *page);
                self.handle_settings(draft, page, event)
            }
            View::Selected => self.handle_selected(event),
            View::Annotation { index } => self.handle_annotation(*index, event),
            View::Note {
                index,
                draft,
                keyboard,
            } => {
                let (index, draft, keyboard) = (*index, draft.clone(), keyboard.clone());
                self.handle_note(index, draft, keyboard, event)
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
            Err(msg) => {
                self.view = View::Message {
                    msg,
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

    /// PDF'in metin katmanı olmayan sayfalarını OCR'dan geçirip görünmez metin
    /// katmanıyla "<ad> - aranabilir.pdf" olarak orijinalin yanına kaydeder.
    /// Orijinal değişmez. OCR sonuçları önbellekten gelir, yoksa şimdi yapılır.
    fn save_searchable(&mut self) -> Msg {
        let Some(s) = &self.session else {
            return Msg::SearchableFailed;
        };
        let Some(pdf) = s.pdf() else {
            return Msg::SearchableFailed;
        };
        let started = std::time::Instant::now();
        let mut edit = match PdfEdit::open(&s.path) {
            Ok(e) => e,
            Err(e) => {
                log::error!("cannot open {} for writing: {e:?}", s.path.display());
                return Msg::SearchableFailed;
            }
        };
        let mut pages = 0;
        for page in 0..pdf.page_count() {
            if !pdf.page_blocks(page).unwrap_or_default().is_empty() {
                continue; // zaten metni var
            }
            let Some(layer) = session::ocr_layer(pdf, s.ocr_cache.as_ref(), page) else {
                continue;
            };
            if layer.words.is_empty() {
                continue;
            }
            if let Err(e) = edit.add_text_layer(&layer) {
                log::error!("cannot add text layer to page {page}: {e:?}");
                return Msg::SearchableFailed;
            }
            pages += 1;
        }
        if pages == 0 {
            return Msg::NothingToOcr;
        }
        let name = format!("{} - aranabilir.pdf", s.name);
        let output = s.path.with_file_name(&name);
        match edit.save(&output, &s.path) {
            Ok(()) => {
                log::info!(
                    "saved {} ({pages} pages) in {:?}",
                    output.display(),
                    started.elapsed()
                );
                Msg::SearchableSaved { pages, name }
            }
            Err(e) => {
                log::error!("cannot save {}: {e:?}", output.display());
                Msg::SearchableFailed
            }
        }
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
            View::Annotations { page } => self.draw_annotations(canvas, *page),
            View::Backup => self.draw_backup(canvas),
            View::Lookup { page } => self.draw_lookup(canvas, *page),
            View::Settings { draft, page } => self.draw_settings(canvas, draft, *page),
            View::Selected | View::Annotation { .. } => {
                self.draw_reading(canvas);
                self.draw_actions(canvas);
            }
            View::Note {
                index,
                draft,
                keyboard,
            } => self.draw_note(canvas, *index, draft, keyboard),
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
