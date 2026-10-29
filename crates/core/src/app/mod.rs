//! Uygulama: ekranlar arası geçiş, olay işleme, kayıt ve çizim.
//!
//! Ekranlar: kütüphane → (kitap) içindekiler / okuma → menü → yer imleri,
//! yazı ayarları. Okurken ekranın ortasına dokunmak menüyü açar. Kütüphaneden
//! OPDS kataloglarına gidilir.
//! PDF'ler aynı ekranlarla okunur; sayfa görüntüsü çizilip önbellekte tutulur.

mod annotations;
mod autoturn;
mod backup;
mod bookcase;
mod catalog;
mod covers;
mod downloads;
mod export;
mod form;
mod images;
mod jobs;
mod keyboard;
mod links;
mod lookup;
mod news;
mod note;
mod orientation;
mod pace;
mod pdf_view;
mod screens;
mod search;
mod select;
mod session;
mod settings;
mod shelf;
mod stats;
mod translate;
mod ui;
mod upload;
mod webdav;
mod words;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::canvas::Canvas;
use crate::epub::Book;
use crate::event::{Event, Gestures, PageKey, Refresh};
use crate::i18n::Msg;
use crate::library::{self, Entry, Format};
use crate::pdf::{GrayImage, PdfDoc, PdfEdit};
use crate::storage::Store;
use crate::text::Text;
pub use annotations::Annotation;
pub use session::{BookInfo, BookState, Bookmark, Position};
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
    /// Araçlar: yedekleme, geri yükleme, istatistikler (kütüphaneden).
    Backup,
    /// Okuma istatistikleri.
    Stats,
    /// Kablosuz yükleme sunucusu açık (`upload`).
    Upload,
    /// WebDAV: yedek gönder/al, gez, hesap.
    WebDav,
    /// WebDAV klasörü (`dav.dirs` sonuncusu).
    DavBrowse {
        page: usize,
    },
    /// Haber beslemeleri (`news`).
    NewsFeeds {
        page: usize,
    },
    /// Beslemenin yazıları.
    NewsItems {
        feed: usize,
        page: usize,
    },
    /// Açık yazı (`news.article` sayfaları).
    NewsArticle {
        feed: usize,
        item: usize,
        page: usize,
    },
    /// Sözlük sonucu (`lookup`), sayfası.
    Lookup {
        page: usize,
    },
    /// Kişisel kelime listesi.
    Words {
        page: usize,
    },
    /// Kelime kartı tekrarı: sorulan kart (`words` sırası; yoksa bugünlük
    /// bitti), çevrildiyse tanımı, bu oturumda bilinen kart sayısı.
    Review {
        card: Option<usize>,
        answer: Option<String>,
        done: u32,
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
    /// OPDS katalogları (kütüphaneden).
    Catalogs {
        page: usize,
    },
    /// Açık katalog sayfası (`browser.feeds` sonuncusu).
    Feed,
    /// Katalog sayfasındaki kitabın ayrıntısı.
    Entry {
        index: usize,
    },
    /// Ağ isteği bekleniyor.
    Loading,
    /// Kelime çevrimiçi sözlükte aranıyor.
    Searching,
    /// Dipnot altta açılır pencerede (`footnote`).
    Footnote,
    /// Kitap içi arama sonuçları (`search`).
    SearchResults {
        page: usize,
    },
    /// Kitabın okuma durumu seçimi (kütüphanede uzun basınca; `library` sırası).
    BookActions {
        book: usize,
    },
    /// Kitap dosyası silinsin mi?
    ConfirmDelete {
        book: usize,
    },
    /// Kitabın taşınacağı klasör seçiliyor.
    MoveBook {
        book: usize,
        page: usize,
    },
    /// Bölümdeki görsel (`blocks` sırası) tam ekran.
    Image {
        block: usize,
    },
    /// İndirme kuyruğu; `back` katalog listesi ya da katalog sayfası.
    Downloads {
        page: usize,
        back: Back,
    },
    Form(Box<form::Form>),
}

/// Mesaj ekranından dokununca dönülecek yer.
#[derive(Clone, Copy)]
enum Back {
    Library,
    Contents,
    Reading,
    Catalogs,
    Feed,
}

pub struct App {
    text: Text,
    /// Geçerli ekran boyutu: dikeyde platformun verdiği genişlik × Libra 2
    /// yüksekliği; okuma yatay döndüyse ikisi yer değiştirir.
    width: u32,
    height: u32,
    /// Platformun verdiği (dikey) ekran genişliği.
    portrait_width: u32,
    /// İvmeölçere göre cihaz yatay tutuluyor.
    tilted: bool,
    /// Kitap kimliği → son açılma ve elle seçilen okuma durumu.
    shelf: HashMap<String, shelf::Shelved>,
    /// Kapak ızgarasında görünen kitapların kapakları (`None`: kapağı yok)
    /// ve eksikleri üreten iş.
    covers: HashMap<String, Option<covers::Thumb>>,
    cover_job: Option<covers::CoverJob>,
    /// Haber beslemeleri ve yazılar.
    news: news::NewsState,
    /// WebDAV hesabı, işi ve açılan klasörleri.
    dav: webdav::Dav,
    /// Kullanıcının DeepL API anahtarı (`deepl.json`); yoksa çeviri yok.
    deepl_key: Option<String>,
    /// Açık kablosuz yükleme sunucusu (yalnızca yükleme ekranındayken).
    upload: Option<upload::Server>,
    /// Okuma istatistikleri ve kaydedilmemiş değişikliği var mı.
    stats: stats::Stats,
    stats_dirty: bool,
    /// Kütüphanede arama (başlık/yazar); yoksa hepsi.
    library_query: Option<String>,
    /// Vurgular ve notlar listesinde arama.
    annotation_query: Option<String>,
    /// Otomatik sayfa çevirme açıksa son çevirmenin anı.
    auto_turn: Option<std::time::Instant>,
    store: Store,
    /// Bütün kitapların varsayılan ayarları (`settings.json`). `settings` bunun
    /// açık kitabın kendi yazı ayarlarıyla birleşmiş hâlidir.
    defaults: Settings,
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
    /// Yatayda yan yana gösterilen sağ PDF sayfası.
    pdf_right: RefCell<Option<PdfFrame>>,
    gestures: Gestures,
    /// Okurken uzun basmayla başlayan seçim.
    selection: Option<select::Selection>,
    /// Sözlükler; ilk aramada açılır.
    dicts: Option<crate::dict::Dictionaries>,
    /// Gösterilen sözlük sonucu.
    lookup: Option<lookup::Lookup>,
    /// Kişisel kelime listesi.
    words: Vec<words::SavedWord>,
    /// OPDS katalogları ve gezinme.
    browser: catalog::Browser,
    /// İndirme kuyruğu (yalnızca uygulama açıkken).
    downloads: Vec<downloads::Download>,
    /// Sürmekte olan çevrimiçi sözlük araması: kelime ve iş.
    online: Option<lookup::OnlineSearch>,
    /// Çevrimiçi sözlüklerin adresleri.
    endpoints: lookup::Endpoints,
    /// Okuma hızı (kalan süre tahmini için) ve kaydedilmemiş değişikliği var mı.
    pace: pace::Pace,
    pace_dirty: bool,
    /// Okunan sayfanın ekrana geldiği an; sayfada geçen süre ölçülür.
    page_shown: Option<std::time::Instant>,
    /// Bağlantı izlenmeden önceki yerler (bölüm, konum); "← Geri" ile dönülür.
    link_back: Vec<(usize, Position)>,
    /// Açılır penceredeki dipnot.
    footnote: Option<links::Footnote>,
    /// Açık kitaptaki son arama ve okunan sayfada işaretli sonuç (bölüm, başı, sonu).
    search: Option<search::BookSearch>,
    search_mark: Option<(usize, Position, Position)>,
    /// Kullanıcı "Çık"a dokundu; kayıtlar yazıldı, platform uygulamayı kapatmalı.
    exit: bool,
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
        let settings: Settings = store.load(SETTINGS_FILE).unwrap_or_default();
        let words = store.load(words::WORDS_FILE).unwrap_or_default();
        let pace = store.load(pace::PACE_FILE).unwrap_or_default();
        let stats = store.load(stats::STATS_FILE).unwrap_or_default();
        let dav_account = store.load(webdav::WEBDAV_FILE);
        let deepl_key = store
            .load::<translate::DeeplKey>(translate::DEEPL_FILE)
            .map(|k| k.key)
            .filter(|k| !k.trim().is_empty());
        let catalogs = store
            .load(catalog::CATALOGS_FILE)
            .unwrap_or_else(catalog::default_catalogs);
        let library = library::scan(library_root, &store);
        let states: Vec<(String, BookState)> = library
            .iter()
            .filter_map(|e| Some((e.id.clone(), store.load::<BookState>(&book_file(&e.id))?)))
            .collect();
        let progress = states
            .iter()
            .map(|(id, s)| (id.clone(), s.percent))
            .collect();
        let shelf = states
            .into_iter()
            .map(|(id, s)| {
                let info = shelf::Shelved {
                    opened: s.opened,
                    status: s.status,
                    info: s.info,
                };
                (id, info)
            })
            .collect();
        text.set_weight(settings.weight);
        crate::i18n::set_language(settings.language);
        let mut app = Self {
            text,
            width,
            height: crate::LIBRA2_HEIGHT,
            portrait_width: width,
            tilted: false,
            auto_turn: None,
            shelf,
            covers: HashMap::new(),
            cover_job: None,
            upload: None,
            deepl_key,
            news: news::NewsState {
                news: store.load(news::NEWS_FILE).unwrap_or_default(),
                ..Default::default()
            },
            dav: webdav::Dav {
                account: dav_account,
                ..Default::default()
            },
            stats,
            stats_dirty: false,
            library_query: None,
            annotation_query: None,
            store,
            defaults: settings.clone(),
            root: library_root.to_path_buf(),
            settings,
            library,
            progress,
            session: None,
            view: View::Library { page: 0 },
            since_full: 0,
            unsaved_turns: 0,
            pdf_frame: RefCell::new(None),
            pdf_right: RefCell::new(None),
            gestures: Gestures::default(),
            selection: None,
            dicts: None,
            lookup: None,
            words,
            browser: catalog::Browser::new(catalogs),
            downloads: Vec::new(),
            online: None,
            endpoints: lookup::Endpoints::default(),
            pace,
            pace_dirty: false,
            page_shown: None,
            link_back: Vec::new(),
            footnote: None,
            search: None,
            search_mark: None,
            exit: false,
        };
        app.apply_book_info();
        app
    }

    /// İlk açılışta ekran her zaman tam yenilenir.
    pub fn start(&mut self) -> Refresh {
        self.request_covers();
        self.refresh(true)
    }

    /// Kullanıcı uygulamadan çıkmak istedi (kayıtlar yazıldı); platform
    /// kendi ekranına dönmeli.
    pub fn wants_exit(&self) -> bool {
        self.exit
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
        let mut state: BookState = self.store.load(&book_file(&id)).unwrap_or_default();
        state.opened = annotations::now();
        self.store.save(&book_file(&id), &state);
        self.shelf.entry(id.clone()).or_default().opened = state.opened;
        let last = state.last;
        self.use_settings(
            self.defaults
                .with_book_fields(state.layout.as_ref().unwrap_or(&self.defaults)),
        );
        let name = path
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.link_back.clear();
        self.search = None;
        self.search_mark = None;
        let mut session = Session::new(doc, name, id.clone(), state);
        session.path = path.to_path_buf();
        if is_pdf {
            session.ocr_cache = Some(Store::new(&self.store.dir().join("ocr").join(&id)));
        }
        self.session = Some(session);
        self.pdf_frame.replace(None);
        self.pdf_right.replace(None);
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
        let recognised = match event {
            Event::Touch { phase, x, y, ms } => Some(self.gestures.feed(phase, x, y, ms)),
            Event::SecondTouch { phase, x, y } => Some(self.gestures.feed_second(phase, x, y)),
            _ => None,
        };
        if let Some(events) = recognised {
            return events
                .into_iter()
                .filter_map(|e| self.handle(e))
                .max_by_key(|r| *r == Refresh::Full);
        }
        if event == Event::Tick {
            let feed = self.poll_job();
            let downloads = self.poll_downloads();
            let online = self.poll_online();
            let turned = self.poll_auto_turn();
            let covers = self.poll_covers();
            let upload = self.poll_upload();
            let dav = self.poll_dav();
            let news = self.poll_news();
            return [feed, downloads, online, turned, covers, upload, dav, news]
                .into_iter()
                .flatten()
                .max_by_key(|r| *r == Refresh::Full);
        }
        // Ayara göre sayfa tuşlarının anlamı ters çevrilir (her ekranda).
        let event = match event {
            Event::Key(k) if self.settings.swap_keys => Event::Key(match k {
                PageKey::Forward => PageKey::Back,
                PageKey::Back => PageKey::Forward,
            }),
            e => e,
        };
        if let Event::Tilt { landscape } = event {
            self.tilted = landscape;
            return self.fit_orientation().then(|| self.refresh(true));
        }
        // Otomatik çevirme sürerken dokunma ya da tuş yalnızca durdurur.
        if self.auto_turn.is_some()
            && matches!(self.view, View::Reading)
            && matches!(
                event,
                Event::Tap { .. } | Event::Key(_) | Event::Swipe { .. } | Event::Hold { .. }
            )
        {
            self.stop_auto_turn();
            return None;
        }
        let refresh = self.dispatch(event);
        self.pause_dwell();
        self.request_covers();
        if self.fit_orientation() {
            // Ekran döndü: her şey yeniden çizilir.
            return Some(self.refresh(true));
        }
        refresh
    }

    fn dispatch(&mut self, event: Event) -> Option<Refresh> {
        match &self.view {
            View::Library { page } => self.handle_library(*page, event),
            View::Message { back, .. } => {
                let back = *back;
                matches!(event, Event::Tap { .. }).then(|| {
                    self.view = match back {
                        Back::Library => View::Library { page: 0 },
                        Back::Contents => View::Contents { page: 0 },
                        Back::Reading => View::Reading,
                        Back::Catalogs => View::Catalogs { page: 0 },
                        Back::Feed => View::Feed,
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
            View::Stats => self.handle_stats(event),
            View::Upload => self.handle_upload(event),
            View::WebDav => self.handle_webdav(event),
            View::DavBrowse { page } => self.handle_dav_browse(*page, event),
            View::NewsFeeds { page } => self.handle_news_feeds(*page, event),
            View::NewsItems { feed, page } => self.handle_news_items(*feed, *page, event),
            View::NewsArticle { feed, item, page } => {
                self.handle_news_article(*feed, *item, *page, event)
            }
            View::Lookup { page } => self.handle_lookup(*page, event),
            View::Words { page } => self.handle_words(*page, event),
            View::Review { card, answer, done } => {
                let (card, answer, done) = (*card, answer.clone(), *done);
                self.handle_review(card, answer, done, event)
            }
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
            View::Catalogs { page } => self.handle_catalogs(*page, event),
            View::Feed => self.handle_feed(event),
            View::Entry { index } => self.handle_entry(*index, event),
            View::Downloads { page, back } => self.handle_downloads(*page, *back, event),
            View::Loading => self.handle_loading(event),
            View::Searching => self.handle_searching(event),
            View::Footnote => self.handle_footnote(event),
            View::SearchResults { page } => self.handle_search_results(*page, event),
            View::Image { .. } => self.handle_image_view(event),
            View::BookActions { book } => self.handle_book_actions(*book, event),
            View::ConfirmDelete { book } => self.handle_confirm_delete(*book, event),
            View::MoveBook { book, page } => self.handle_move_book(*book, *page, event),
            View::Form(form) => {
                let form = (**form).clone();
                self.handle_form(form, event)
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
        match session.load_chapter(
            &self.text,
            &self.settings,
            (self.width, self.height),
            spine,
            target,
        ) {
            Ok(()) => {
                self.view = View::Reading;
                self.page_shown = Some(std::time::Instant::now());
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
        // Okunmakta olan sayfanın süresi de kaydedilsin.
        self.flush_reading(false);
        self.save_stats();
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
        if self.pace_dirty {
            self.pace_dirty = false;
            self.store.save(pace::PACE_FILE, &self.pace);
        }
        self.store.save(&book_file(&session.id), &session.state);
        self.progress
            .insert(session.id.clone(), session.state.percent);
        self.save_stats();
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

    /// Geçerli ayarları değiştirir (dizilmiş bölüme dokunmaz).
    fn use_settings(&mut self, settings: Settings) {
        self.settings = settings;
        self.text.set_weight(self.settings.weight);
        crate::i18n::set_language(self.settings.language);
    }

    /// Ayarlar ekranında "Tamam": kitap açıksa yazı ayarları yalnız bu kitaba
    /// uygulanır, genel ayarlar (gece modu, dokunma…) varsayılana yazılır.
    fn apply_settings(&mut self, settings: Settings) {
        if settings == self.settings {
            return;
        }
        match self.session.as_mut() {
            Some(s) => {
                s.state.layout = Some(settings.clone());
                self.defaults = settings.with_book_fields(&self.defaults);
            }
            None => self.defaults = settings.clone(),
        }
        self.store.save(SETTINGS_FILE, &self.defaults);
        self.relayout_with(settings);
    }

    /// Ayarlar ekranında "Varsayılan yap": bu ayarlar bütün kitapların
    /// varsayılanı olur, kitabın kendi ayarı silinir.
    fn make_default(&mut self, settings: Settings) {
        if let Some(s) = self.session.as_mut() {
            s.state.layout = None;
        }
        self.defaults = settings.clone();
        self.store.save(SETTINGS_FILE, &self.defaults);
        self.relayout_with(settings);
    }

    fn relayout_with(&mut self, settings: Settings) {
        self.use_settings(settings);
        if let Some(session) = self.session.as_mut() {
            session.relayout(&self.text, &self.settings, (self.width, self.height));
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
            View::Stats => self.draw_stats(canvas),
            View::Upload => self.draw_upload(canvas),
            View::WebDav => self.draw_webdav(canvas),
            View::DavBrowse { page } => self.draw_dav_browse(canvas, *page),
            View::NewsFeeds { page } => self.draw_news_feeds(canvas, *page),
            View::NewsItems { feed, page } => self.draw_news_items(canvas, *feed, *page),
            View::NewsArticle { page, .. } => self.draw_news_article(canvas, *page),
            View::Lookup { page } => self.draw_lookup(canvas, *page),
            View::Words { page } => self.draw_words(canvas, *page),
            View::Review { card, answer, done } => {
                self.draw_review(canvas, *card, answer.as_deref(), *done)
            }
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
            View::Catalogs { page } => self.draw_catalogs(canvas, *page),
            View::Feed => self.draw_feed(canvas),
            View::Entry { index } => self.draw_entry(canvas, *index),
            View::Loading => self.draw_loading(canvas),
            View::Searching => self.draw_searching(canvas),
            View::Footnote => {
                self.draw_reading(canvas);
                self.draw_footnote(canvas);
            }
            View::SearchResults { page } => self.draw_search_results(canvas, *page),
            View::Image { block } => self.draw_image_view(canvas, *block),
            View::BookActions { book } => self.draw_book_actions(canvas, *book),
            View::ConfirmDelete { book } => self.draw_confirm_delete(canvas, *book),
            View::MoveBook { book, page } => self.draw_move_book(canvas, *book, *page),
            View::Downloads { page, .. } => self.draw_downloads(canvas, *page),
            View::Form(form) => self.draw_form(canvas, form),
        }
        if self.settings.night {
            canvas.invert();
        }
    }
}

impl App {
    /// Platform kapanırken son ekran: cihazın yeniden başlatıldığını söyler
    /// (E Ink'te görüntü kalır; boş kalırsa donmuş sanılır).
    pub fn draw_farewell(&self, canvas: &mut Canvas) {
        canvas.fill(255);
        self.draw_message_lines(canvas, &Msg::Restarting, ui::TOP + 300);
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
