//! Sözlükte arama ekranı: seçilen kelime (ya da kökü) bütün sözlüklerde
//! aranır, maddeler okuyucunun dizgisiyle tam ekran gösterilir. Kelime
//! cihazdaki sözlüklerde yoksa ve ayarlarda açıksa çevrimiçi sözlüklere
//! (TDK, Vikisözlük) arka planda sorulur; bulunan sonuçlar diske
//! önbelleklenir, sonra çevrimdışı da açılır.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::jobs::Job;
use super::session::{Page, paginate};
use super::ui::{MARGIN, ROW_PX, draw_button, draw_footer, draw_header};
use super::{App, Back, View};
use crate::canvas::Canvas;
use crate::dict::{Article, Definition, Dictionaries, online, render, stem};
use crate::epub::{Block, BlockKind};
use crate::event::{Event, PageKey, Refresh};
use crate::i18n::{Msg, tr};
use crate::net::Net;
use crate::storage::Store;
use crate::text::{Span, Style};

/// Kütüphane köküne göre sözlük klasörleri: KOReader'ın sözlükleri, Kobo'nun
/// (yalnızca şifresiz olanlar açılır) ve bizimki.
const DICT_DIRS: [&str; 3] = [".adds/koreader/data/dict", ".kobo/dict", "Sözlükler"];

/// Çevrimiçi yanıtın en büyük boyutu.
const ONLINE_LIMIT: u64 = 2 << 20;
/// Veri klasöründe çevrimiçi sonuçların önbelleği.
const ONLINE_CACHE: &str = "cevrimici-sozluk";

/// Çevrimiçi sözlüklerin adresleri (testlerde yerel sunucu).
#[derive(Debug, Clone)]
pub(super) struct Endpoints {
    pub tdk: String,
    pub wiktionary: String,
    pub wikipedia: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            tdk: online::TDK.into(),
            wiktionary: online::WIKTIONARY.into(),
            wikipedia: online::WIKIPEDIA.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Provider {
    Tdk,
    Wiktionary,
}

impl Provider {
    fn source(self) -> Msg {
        match self {
            Provider::Tdk => Msg::SourceTdk,
            Provider::Wiktionary => Msg::SourceWiktionary,
        }
    }

    /// Önbellekteki kaydın adı: sağlayıcı ve kelimenin anahtarı (dosya adına
    /// uymayan karakterler `_`).
    fn cache_file(self, word: &str) -> String {
        let dir = match self {
            Provider::Tdk => "tdk",
            Provider::Wiktionary => "vikisozluk",
        };
        let name: String = crate::dict::key(word)
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect();
        format!("{ONLINE_CACHE}/{dir}/{name}.json")
    }

    /// Ayrı iş parçacığında: bulunan biçim ve maddeler.
    /// Ayrı iş parçacığında: bulunan biçim ve maddeler. `hint` (başka
    /// sözlüğün bulduğu kök) TDK'da ilk aday olarak denenir.
    fn fetch(
        self,
        net: &Net,
        at: &Endpoints,
        word: &str,
        hint: Option<&str>,
    ) -> Result<Option<Cached>, Msg> {
        let get = |url: &str| {
            net.get(url, None, ONLINE_LIMIT).map_err(|e| {
                log::warn!("online dictionary {url}: {e:?}");
                e.message()
            })
        };
        let invalid = |e: crate::dict::DictError| {
            log::warn!("online dictionary answer: {e:?}");
            Msg::NetInvalid
        };
        let candidates = stem::candidates(word);
        match self {
            Provider::Tdk => {
                let hinted = hint.map(str::to_string).into_iter();
                let mut tried = std::collections::HashSet::new();
                let ordered = hinted.chain(candidates).filter(|c| tried.insert(c.clone()));
                for c in ordered.take(online::TDK_TRIES) {
                    let articles =
                        online::parse_tdk(&get(&online::tdk_url(&at.tdk, &c))?).map_err(invalid)?;
                    if !articles.is_empty() {
                        return Ok(Some(Cached { form: c, articles }));
                    }
                }
                Ok(None)
            }
            Provider::Wiktionary => {
                let lemmas = get(&online::wiktionary_lemma_url(&at.wiktionary, &candidates))?;
                let Some((form, title)) =
                    online::pick_lemma(&lemmas, &candidates).map_err(invalid)?
                else {
                    return Ok(None);
                };
                let page = get(&online::wiktionary_extract_url(&at.wiktionary, &title))?;
                Ok(online::parse_wiktionary(&page, &title)
                    .map_err(invalid)?
                    .map(|a| Cached {
                        form,
                        articles: vec![a],
                    }))
            }
        }
    }
}

/// Sürmekte olan çevrimiçi arama: kelime ve arka plan işi.
pub(super) struct OnlineSearch {
    pub word: String,
    job: Job<Result<Option<Definition>, Msg>>,
    /// Vikipedi araması (sözlük değil): bulunamadı/ulaşılamadı mesajları farklı.
    wikipedia: bool,
}

/// Ayrı iş parçacığında: Vikipedi'de en uygun madde ve özeti.
fn define_wikipedia(net: &Net, at: &Endpoints, query: &str) -> Result<Option<Definition>, Msg> {
    let get = |url: &str| {
        net.get(url, None, ONLINE_LIMIT).map_err(|e| {
            log::warn!("wikipedia {url}: {e:?}");
            e.message()
        })
    };
    let invalid = |e: crate::dict::DictError| {
        log::warn!("wikipedia answer: {e:?}");
        Msg::NetInvalid
    };
    let found =
        online::parse_wikipedia_search(&get(&online::wikipedia_search_url(&at.wikipedia, query))?)
            .map_err(invalid)?;
    let Some(title) = found else {
        return Ok(None);
    };
    let summary = get(&online::wikipedia_summary_url(&at.wikipedia, &title))?;
    Ok(online::parse_wikipedia_summary(&summary)
        .map_err(invalid)?
        .map(|article| Definition {
            form: title,
            articles: vec![(tr(&Msg::SourceWikipedia), article)],
        }))
}

/// Önbellekteki çevrimiçi sonuç.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Cached {
    form: String,
    articles: Vec<Article>,
}

/// Ayrı iş parçacığında: `word`'ü sağlayıcılarda (önce önbellekte) arar.
/// Hiçbiri bulamadıysa ve biri hata verdiyse hatası döner.
fn define_online(
    net: &Net,
    at: &Endpoints,
    providers: &[Provider],
    word: &str,
    data: &Path,
) -> Result<Option<Definition>, Msg> {
    let cache = Store::new(data);
    let mut found = Vec::new();
    let mut error = None;
    // Vikisözlük kök bulmada TDK'dan iyidir (bütün adaylara tek istekte
    // bakar): önce ona sorulur, bulduğu kök TDK'da ilk denenir.
    let mut order = providers.to_vec();
    order.sort_by_key(|p| *p != Provider::Wiktionary);
    for p in order {
        let file = p.cache_file(word);
        if let Some(c) = cache.load::<Cached>(&file) {
            found.push((p, c));
            continue;
        }
        let hint = found
            .first()
            .map(|(_, c): &(Provider, Cached)| c.form.clone());
        match p.fetch(net, at, word, hint.as_deref()) {
            Ok(Some(c)) => {
                cache.save(&file, &c);
                found.push((p, c));
            }
            Ok(None) => {}
            Err(msg) => {
                error.get_or_insert(msg);
            }
        }
    }
    // Gösterim sırası ayarlardaki gibi (TDK önce).
    found.sort_by_key(|(p, _)| providers.iter().position(|q| q == p));
    let Some(form) = found.first().map(|(_, c)| c.form.clone()) else {
        return error.map_or(Ok(None), Err);
    };
    let articles = found
        .into_iter()
        .flat_map(|(p, c)| {
            let source = tr(&p.source());
            c.articles.into_iter().map(move |a| (source.clone(), a))
        })
        .collect();
    Ok(Some(Definition { form, articles }))
}

/// Gösterilen sözlük sonucu, sayfalara dizilmiş.
pub(super) struct Lookup {
    pub pages: Vec<Page>,
    /// Sözlükte bulunan biçim ve metindeki biçim.
    pub form: String,
    pub seen: String,
}

impl App {
    /// Sözlükler ilk aramada açılır (dizin kurmak büyük sözlükte zaman alır).
    pub(super) fn dictionaries(&mut self) -> &Dictionaries {
        self.dicts.get_or_insert_with(|| {
            let dirs: Vec<PathBuf> = DICT_DIRS.iter().map(|d| self.root.join(d)).collect();
            let started = std::time::Instant::now();
            let (dicts, failed) = Dictionaries::open(&dirs);
            for (path, e) in failed {
                log::warn!("cannot open dictionary {}: {e:?}", path.display());
            }
            log::info!(
                "dictionaries {:?} opened in {:?}",
                dicts.names(),
                started.elapsed()
            );
            dicts
        })
    }

    /// Ayarlarda açık çevrimiçi sözlükler (internet kapalıysa hiçbiri).
    fn online_providers(&self) -> Vec<Provider> {
        let s = &self.settings;
        let mut out = Vec::new();
        if s.network && s.online_tdk {
            out.push(Provider::Tdk);
        }
        if s.network && s.online_wiktionary {
            out.push(Provider::Wiktionary);
        }
        out
    }

    /// `word`'ü arar; bulunursa tanım ekranını, bulunamazsa açıklamayı gösterir.
    /// Cihazdaki sözlüklerde yoksa çevrimiçi arama arka planda başlar.
    pub(super) fn look_up(&mut self, word: &str) -> Option<Refresh> {
        let word = word.trim().to_string();
        let local = {
            let dicts = self.dictionaries();
            (!dicts.is_empty()).then(|| dicts.define(&word)).flatten()
        };
        if let Some(def) = local {
            return self.show_definition(&word, &def);
        }
        let providers = self.online_providers();
        if !providers.is_empty() {
            let net = self.browser.net();
            let data = self.store.dir().to_path_buf();
            let (w, at) = (word.clone(), self.endpoints.clone());
            let job = Job::spawn(move |_| define_online(&net, &at, &providers, &w, &data));
            self.online = Some(OnlineSearch {
                word,
                job,
                wikipedia: false,
            });
            self.view = View::Searching;
            return Some(self.refresh(true));
        }
        let msg = if self.dictionaries().is_empty() {
            Msg::NoDictionaries
        } else {
            Msg::NotInDictionary(word)
        };
        self.show_message(msg);
        Some(self.refresh(true))
    }

    /// `Event::Tick`: çevrimiçi arama bittiyse sonucu gösterir.
    pub(super) fn poll_online(&mut self) -> Option<Refresh> {
        let result = self.online.as_ref()?.job.poll()?;
        let OnlineSearch {
            word, wikipedia, ..
        } = self.online.take()?;
        match result {
            Some(Ok(Some(def))) => return self.show_definition(&word, &def),
            Some(Ok(None)) if wikipedia => self.show_message(Msg::NotOnWikipedia(word)),
            Some(Ok(None)) => self.show_message(Msg::NotInDictionary(word)),
            failed => {
                let reason = tr(&failed.and_then(Result::err).unwrap_or(Msg::NetInvalid));
                self.show_message(if wikipedia {
                    Msg::WikipediaFailed(reason)
                } else {
                    Msg::OnlineFailed { word, reason }
                });
            }
        }
        Some(self.refresh(true))
    }

    /// Seçili metni Türkçe Vikipedi'de arar (arka planda; sonuç tanım ekranında).
    pub(super) fn look_up_wikipedia(&mut self, query: &str) -> Option<Refresh> {
        let query = query.trim().to_string();
        if !self.settings.network {
            self.show_message(Msg::NetworkOff);
            return Some(self.refresh(true));
        }
        let net = self.browser.net();
        let (q, at) = (query.clone(), self.endpoints.clone());
        let job = Job::spawn(move |_| define_wikipedia(&net, &at, &q));
        self.online = Some(OnlineSearch {
            word: query,
            job,
            wikipedia: true,
        });
        self.view = View::Searching;
        Some(self.refresh(true))
    }

    pub(super) fn handle_searching(&mut self, event: Event) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        if !self.loading_cancel().contains(x, y) {
            return None;
        }
        self.online = None;
        self.view = View::Reading;
        Some(self.refresh(true))
    }

    pub(super) fn draw_searching(&self, canvas: &mut Canvas) {
        let word = self.online.as_ref().map_or("", |o| o.word.as_str());
        draw_header(
            &self.text,
            canvas,
            &tr(&Msg::SearchingDictionary),
            word,
            false,
        );
        draw_button(
            &self.text,
            canvas,
            self.loading_cancel(),
            &tr(&Msg::Cancel),
            ROW_PX,
        );
    }

    /// Sözlük sonucunu sayfalara dizip tanım ekranını açar.
    fn show_definition(&mut self, word: &str, def: &Definition) -> Option<Refresh> {
        let width = self.width;
        let mut blocks = Vec::new();
        let asked = crate::dict::key(word);
        if crate::dict::key(&def.form) != asked {
            blocks.push(italic(&tr(&Msg::LookupForm {
                word: word.trim().to_string(),
                form: def.form.clone(),
            })));
        }
        for (dict, article) in &def.articles {
            blocks.extend(render::article_blocks(dict, article));
        }
        let pages = paginate(
            &self.text,
            &self.settings,
            Some("tr"),
            (width, self.height),
            &blocks,
        );
        self.lookup = Some(Lookup {
            pages,
            form: def.form.clone(),
            seen: word.to_string(),
        });
        self.view = View::Lookup { page: 0 };
        Some(self.refresh(true))
    }

    fn show_message(&mut self, msg: Msg) {
        self.view = View::Message {
            msg,
            back: Back::Reading,
        };
    }

    pub(super) fn handle_lookup(&mut self, page: usize, event: Event) -> Option<Refresh> {
        let pages = self.lookup.as_ref().map_or(0, |l| l.pages.len());
        if let Event::Tap { x, y } = event
            && self.word_button().contains(x, y)
            && let Some(l) = &self.lookup
        {
            let (form, seen) = (l.form.clone(), l.seen.clone());
            self.toggle_saved(&form, &seen);
            return Some(self.refresh(false));
        }
        let third = self.width / 3;
        let next = match event {
            Event::Key(PageKey::Forward) => page + 1,
            Event::Key(PageKey::Back) => page.wrapping_sub(1),
            Event::Tap { x, .. } if x < third => page.wrapping_sub(1),
            Event::Tap { x, .. } if x > 2 * third => page + 1,
            Event::Tap { .. } => {
                self.lookup = None;
                self.view = View::Reading;
                return Some(self.refresh(true));
            }
            _ => return None,
        };
        if next >= pages {
            return None;
        }
        self.view = View::Lookup { page: next };
        Some(self.refresh(false))
    }

    pub(super) fn draw_lookup(&self, canvas: &mut Canvas, page: usize) {
        let Some(l) = &self.lookup else {
            return;
        };
        if let Some(p) = l.pages.get(page) {
            for placed in &p.lines {
                self.text
                    .draw_line(canvas, &placed.line, self.settings.margin, placed.baseline);
            }
        }
        let footer = Msg::LookupFooter {
            page: page + 1,
            pages: l.pages.len(),
        };
        draw_footer(&self.text, canvas, &footer, MARGIN);
        self.draw_word_button(canvas, &l.form);
    }
}

fn italic(text: &str) -> Block {
    Block {
        kind: BlockKind::Paragraph,
        text: text.to_string(),
        spans: vec![Span {
            start: 0,
            end: text.len(),
            style: Style {
                italic: true,
                ..Style::default()
            },
        }],
        ..Block::default()
    }
}
