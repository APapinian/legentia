//! OPDS katalogları: katalog listesi (`catalogs.json`), katalog sayfalarında
//! gezinme, arama ve kitap ayrıntısı (indirme: `downloads`). Ağ isteği arka planda (`Job`) yürür;
//! sonuç `Event::Tick` geldikçe yoklanır, beklerken "Vazgeç" ile bırakılabilir.

use serde::{Deserialize, Serialize};

use super::form::{Button, Field, Form, FormKind};
use super::jobs::Job;
use super::ui::{
    BOLD, HEADER_BOTTOM, List, MARGIN, PLAIN, ROW_PX, Rect, back_button, draw_button, draw_footer,
    draw_header, draw_header_until, header_button, one_line,
};
use super::{App, Back, View};
use crate::canvas::Canvas;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};
use crate::library::Format;
use crate::net::{Auth, Net};
use crate::opds::{self, EntryKind, Feed, Search};
use crate::text::Family;

pub(super) const CATALOGS_FILE: &str = "catalogs.json";
/// Katalog sayfası ve OpenSearch açıklaması için boyut sınırları.
const FEED_LIMIT: u64 = 4 << 20;
const DESCRIPTION_LIMIT: u64 = 256 << 10;
pub(super) const CATALOGS: List = List {
    top: HEADER_BOTTOM,
    row_h: 120,
};
pub(super) const FEED: List = List {
    top: HEADER_BOTTOM,
    row_h: 120,
};
/// Başlıktaki "Ekle" / "Ara" düğmesinin genişliği.
const HEADER_BUTTON_W: i32 = 170;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub auth: Option<Auth>,
    /// Katalog üyelik istiyor (listede belirtilir).
    #[serde(default)]
    pub members_only: bool,
}

pub(super) fn default_catalogs() -> Vec<Catalog> {
    vec![
        Catalog {
            name: "Project Gutenberg".into(),
            url: "https://www.gutenberg.org/ebooks.opds/".into(),
            auth: None,
            members_only: false,
        },
        Catalog {
            name: "Standard Ebooks".into(),
            url: "https://standardebooks.org/feeds/opds".into(),
            auth: None,
            members_only: true,
        },
    ]
}

/// Açılmış bir katalog sayfası (gezinme yığınının bir katı).
pub(super) struct FeedPage {
    pub feed: Feed,
    /// Listede gösterilen sayfa.
    pub page: usize,
}

/// İstenen katalog sayfası.
pub(super) enum Request {
    Feed(String),
    Search { search: Search, query: String },
}

type Loaded = Result<FeedPage, Msg>;

/// Katalog durumu: liste, açık katalog ve sürmekte olan istek.
pub(super) struct Browser {
    pub catalogs: Vec<Catalog>,
    /// İlk istekte kurulur.
    net: Option<Net>,
    /// Açık katalogun sırası.
    pub current: usize,
    /// Açılan sayfalar; sonuncusu gösterilir. Boşsa katalog listesindeyiz.
    pub feeds: Vec<FeedPage>,
    pub job: Option<Job<Loaded>>,
}

impl Browser {
    pub fn new(catalogs: Vec<Catalog>) -> Browser {
        Browser {
            catalogs,
            net: None,
            current: 0,
            feeds: Vec::new(),
            job: None,
        }
    }

    pub fn net(&mut self) -> Net {
        self.net.get_or_insert_with(Net::default).clone()
    }

    fn feed(&self) -> Option<&FeedPage> {
        self.feeds.last()
    }

    /// Gösterilen sayfanın satırları: girdiler ve varsa "Sonraki sayfa".
    fn rows(&self) -> usize {
        self.feed()
            .map_or(0, |f| f.feed.entries.len() + f.feed.next.is_some() as usize)
    }
}

/// Şema ve sunucu adı (`https://ornek.org:8080`), küçük harfle.
fn origin(url: &str) -> String {
    let start = url.find("://").map_or(0, |i| i + 3);
    let end = url[start..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |i| start + i);
    url[..end].to_ascii_lowercase()
}

/// Kullanıcı adı ve şifre yalnızca katalogun kendi sunucusuna gönderilir;
/// katalog başka sunucudaki bir adrese bağlantı verse de oraya gitmez.
pub(super) fn auth_for<'a>(catalog: &'a Catalog, url: &str) -> Option<&'a Auth> {
    catalog
        .auth
        .as_ref()
        .filter(|_| origin(url) == origin(&catalog.url))
}

/// Ayrı iş parçacığında çalışır: sayfayı (aramada önce gerekirse OpenSearch
/// açıklamasını) indirip ayrıştırır.
fn fetch(net: &Net, catalog: &Catalog, request: Request) -> Loaded {
    let get = |url: &str, limit| {
        net.get(url, auth_for(catalog, url), limit).map_err(|e| {
            log::warn!("cannot fetch {url}: {e:?}");
            e.message()
        })
    };
    let url = match request {
        Request::Feed(url) => url,
        Request::Search { search, query } => {
            let template = match search {
                Search::Template(t) => t,
                Search::Description(url) => {
                    let xml = get(&url, DESCRIPTION_LIMIT)?;
                    opds::opensearch_template(&xml, &url).ok_or(Msg::SearchUnsupported)?
                }
            };
            opds::search_url(&template, &query)
        }
    };
    let xml = get(&url, FEED_LIMIT)?;
    let feed = opds::parse(&xml, &url).map_err(|e| {
        log::warn!("not an OPDS feed {url}: {e:?}");
        Msg::NotOpds
    })?;
    Ok(FeedPage { feed, page: 0 })
}

/// Kısaltılmış adresi tamamlar: şema yoksa `http://`, yol yoksa Calibre'nin
/// OPDS yolu `/opds` eklenir ("192.168.1.5:8080" → "http://192.168.1.5:8080/opds").
fn complete_url(url: &str) -> String {
    let url = if url.contains("://") {
        url.to_string()
    } else {
        format!("http://{url}")
    };
    let host = url.find("://").map_or(0, |i| i + 3);
    if url[host..].contains(['/', '?']) {
        url
    } else {
        format!("{url}/opds")
    }
}

/// Formdan girilen katalog; geçersizse nedeni.
fn catalog_from(form: &Form, members_only: bool) -> Result<Catalog, Msg> {
    let value = |i: usize| form.fields[i].value.trim().to_string();
    let (name, url, user) = (value(0), complete_url(&value(1)), value(2));
    if name.is_empty() {
        return Err(Msg::NameRequired);
    }
    let host = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or("");
    if host.is_empty() || host.starts_with('/') || url.contains(' ') {
        return Err(Msg::InvalidUrl);
    }
    Ok(Catalog {
        name,
        url,
        auth: (!user.is_empty()).then(|| Auth {
            user,
            password: form.fields[3].value.clone(),
        }),
        members_only,
    })
}

impl App {
    fn save_catalogs(&mut self) {
        self.store.save(CATALOGS_FILE, &self.browser.catalogs);
    }

    pub(super) fn open_catalogs(&mut self) -> Option<Refresh> {
        if !self.settings.network {
            self.view = View::Message {
                msg: Msg::NetworkOff,
                back: Back::Library,
            };
        } else {
            self.browser.feeds.clear();
            self.view = View::Catalogs { page: 0 };
        }
        Some(self.refresh(true))
    }

    /// İsteği arka planda başlatır; bekleme ekranına geçer.
    fn load(&mut self, request: Request) -> Option<Refresh> {
        let catalog = self.browser.catalogs.get(self.browser.current)?.clone();
        let net = self.browser.net();
        self.browser.job = Some(Job::spawn(move |_| fetch(&net, &catalog, request)));
        self.view = View::Loading;
        Some(self.refresh(true))
    }

    /// Bekleme bittiyse ya da vazgeçildiyse dönülecek ekran.
    fn browse_view(&self) -> View {
        if self.browser.feeds.is_empty() {
            View::Catalogs { page: 0 }
        } else {
            View::Feed
        }
    }

    /// `Event::Tick`: istek bittiyse sonucu gösterir.
    pub(super) fn poll_job(&mut self) -> Option<Refresh> {
        let result = self.browser.job.as_ref()?.poll()?;
        self.browser.job = None;
        match result {
            Some(Ok(page)) => {
                self.browser.feeds.push(page);
                self.view = View::Feed;
            }
            failed => {
                let msg = failed.and_then(Result::err).unwrap_or(Msg::NetInvalid);
                let back = if self.browser.feeds.is_empty() {
                    Back::Catalogs
                } else {
                    Back::Feed
                };
                self.view = View::Message { msg, back };
            }
        }
        Some(self.refresh(true))
    }

    pub(super) fn handle_loading(&mut self, event: Event) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        if !self.loading_cancel().contains(x, y) {
            return None;
        }
        if let Some(job) = self.browser.job.take() {
            job.cancel();
        }
        self.view = self.browse_view();
        Some(self.refresh(true))
    }

    pub(super) fn loading_cancel(&self) -> Rect {
        Rect {
            x: MARGIN,
            y: HEADER_BOTTOM + 200,
            w: self.width as i32 - 2 * MARGIN,
            h: 110,
        }
    }

    pub(super) fn draw_loading(&self, canvas: &mut Canvas) {
        let t = &self.text;
        let name = self
            .browser
            .catalogs
            .get(self.browser.current)
            .map_or(String::new(), |c| c.name.clone());
        draw_header(t, canvas, &tr(&Msg::Loading), &name, false);
        draw_button(t, canvas, self.loading_cancel(), &tr(&Msg::Cancel), ROW_PX);
    }

    // --- Katalog listesi ---

    fn catalog_form(&self, kind: FormKind) -> Form {
        let c = match kind {
            FormKind::EditCatalog(i) => self.browser.catalogs.get(i).cloned(),
            _ => None,
        };
        let auth = c.as_ref().and_then(|c| c.auth.clone());
        let mut password = Field::new(
            Msg::FieldPassword,
            auth.as_ref().map_or("", |a| a.password.as_str()),
        );
        password.secret = true;
        let fields = vec![
            Field::new(Msg::FieldName, c.as_ref().map_or("", |c| c.name.as_str())),
            Field::new(Msg::FieldUrl, c.as_ref().map_or("", |c| c.url.as_str())),
            Field::new(
                Msg::FieldUser,
                auth.as_ref().map_or("", |a| a.user.as_str()),
            ),
            password,
        ];
        Form::new(kind, fields)
    }

    pub(super) fn handle_catalogs(&mut self, page: usize, event: Event) -> Option<Refresh> {
        let len = self.browser.catalogs.len();
        match event {
            Event::Tap { x, y } => {
                if back_button(self.width).contains(x, y) {
                    self.view = View::Library { page: 0 };
                    return Some(self.refresh(true));
                }
                if self
                    .header_buttons(true)
                    .1
                    .is_some_and(|r| r.contains(x, y))
                {
                    self.view = View::Downloads {
                        page: 0,
                        back: Back::Catalogs,
                    };
                    return Some(self.refresh(true));
                }
                if header_button(self.width, HEADER_BUTTON_W).contains(x, y) {
                    return self.open_form(self.catalog_form(FormKind::NewCatalog));
                }
                let index = CATALOGS.row_at(page, len, y)?;
                self.browser.current = index;
                self.browser.feeds.clear();
                let url = self.browser.catalogs[index].url.clone();
                self.load(Request::Feed(url))
            }
            Event::Hold { y, .. } => {
                let index = CATALOGS.row_at(page, len, y)?;
                self.open_form(self.catalog_form(FormKind::EditCatalog(index)))
            }
            _ => {
                let page = Self::page_list(page, CATALOGS.pages(len), event)?;
                self.view = View::Catalogs { page };
                Some(self.refresh(false))
            }
        }
    }

    pub(super) fn draw_catalogs(&self, canvas: &mut Canvas, page: usize) {
        let t = &self.text;
        let catalogs = &self.browser.catalogs;
        self.draw_browse_header(
            canvas,
            &tr(&Msg::Catalogs),
            &tr(&Msg::CatalogCount(catalogs.len())),
            Some(Msg::AddCatalog),
        );
        if catalogs.is_empty() {
            self.draw_message_lines(canvas, &Msg::NoCatalogs, HEADER_BOTTOM + 80);
        }
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
        for (index, row) in CATALOGS.visible(page, catalogs.len()) {
            let c = &catalogs[index];
            let r = CATALOGS.row_rect(canvas.width(), row);
            let name = one_line(t, &c.name, BOLD, ROW_PX, max_w);
            t.draw_plain(canvas, &name, Family::Ui, BOLD, MARGIN, r.y + 50, ROW_PX);
            let second = if c.members_only {
                format!("{} · {}", c.url, tr(&Msg::MembersOnly))
            } else {
                c.url.clone()
            };
            let second = one_line(t, &second, PLAIN, 30.0, max_w);
            t.draw_plain(canvas, &second, Family::Ui, PLAIN, MARGIN, r.y + 95, 30.0);
        }
        let footer = Msg::ListFooter {
            page: page + 1,
            pages: CATALOGS.pages(catalogs.len()),
        };
        draw_footer(t, canvas, &footer, MARGIN);
    }

    /// Form düğmesine dokunuldu.
    pub(super) fn form_button(&mut self, mut form: Form, button: Button) -> Option<Refresh> {
        let catalogs_view = View::Catalogs { page: 0 };
        match (button, form.kind.clone()) {
            (Button::Cancel, FormKind::Search(_)) => self.view = View::Feed,
            (Button::Cancel, _) => self.view = catalogs_view,
            (Button::Delete, FormKind::EditCatalog(i)) => {
                if i < self.browser.catalogs.len() {
                    self.browser.catalogs.remove(i);
                    self.save_catalogs();
                }
                self.view = catalogs_view;
            }
            (Button::Submit, FormKind::Search(search)) => {
                let query = form.fields[0].value.trim().to_string();
                if query.is_empty() {
                    form.error = Some(Msg::QueryRequired);
                    return self.open_form(form);
                }
                return self.load(Request::Search { search, query });
            }
            (Button::Submit, kind) => {
                let index = match kind {
                    FormKind::EditCatalog(i) => Some(i),
                    _ => None,
                };
                let members_only = index
                    .and_then(|i| self.browser.catalogs.get(i))
                    .is_some_and(|c| c.members_only);
                match catalog_from(&form, members_only) {
                    Ok(c) => {
                        match index.filter(|&i| i < self.browser.catalogs.len()) {
                            Some(i) => self.browser.catalogs[i] = c,
                            None => self.browser.catalogs.push(c),
                        }
                        self.save_catalogs();
                        self.view = catalogs_view;
                    }
                    Err(msg) => {
                        form.error = Some(msg);
                        return self.open_form(form);
                    }
                }
            }
            (Button::Delete, _) => return None,
        }
        Some(self.refresh(true))
    }

    // --- Katalog sayfası ---

    fn search_button(&self) -> Option<Rect> {
        self.browser.feed()?.feed.search.as_ref()?;
        Some(header_button(self.width, HEADER_BUTTON_W))
    }

    /// Başlıktaki ek düğme ("Ekle" ya da "Ara", `extra` ise) ve varsa "İndirmeler".
    fn header_buttons(&self, extra: bool) -> (Option<Rect>, Option<Rect>) {
        let extra = extra.then(|| header_button(self.width, HEADER_BUTTON_W));
        let downloads = self.downloads_button(extra.unwrap_or(back_button(self.width)));
        (extra, downloads)
    }

    /// Başlık; sağda (sağdan sola) "Geri", `extra` düğmesi ve varsa "İndirmeler".
    fn draw_browse_header(
        &self,
        canvas: &mut Canvas,
        title: &str,
        subtitle: &str,
        extra: Option<Msg>,
    ) {
        let t = &self.text;
        let back = back_button(canvas.width());
        let (extra_rect, downloads) = self.header_buttons(extra.is_some());
        let left = downloads.or(extra_rect).unwrap_or(back);
        draw_header_until(t, canvas, title, subtitle, left.x - 30);
        draw_button(t, canvas, back, &tr(&Msg::Back), ROW_PX);
        if let (Some(r), Some(label)) = (extra_rect, extra) {
            draw_button(t, canvas, r, &tr(&label), ROW_PX);
        }
        if let Some(r) = downloads {
            self.draw_downloads_button(canvas, r);
        }
    }

    pub(super) fn handle_feed(&mut self, event: Event) -> Option<Refresh> {
        let rows = self.browser.rows();
        let page = self.browser.feed()?.page;
        let Event::Tap { x, y } = event else {
            let page = Self::page_list(page, FEED.pages(rows), event)?;
            self.browser.feeds.last_mut()?.page = page;
            return Some(self.refresh(false));
        };
        if back_button(self.width).contains(x, y) {
            self.browser.feeds.pop();
            self.view = self.browse_view();
            return Some(self.refresh(true));
        }
        let searchable = self.search_button().is_some();
        if self
            .header_buttons(searchable)
            .1
            .is_some_and(|r| r.contains(x, y))
        {
            self.view = View::Downloads {
                page: 0,
                back: Back::Feed,
            };
            return Some(self.refresh(true));
        }
        if self.search_button().is_some_and(|r| r.contains(x, y)) {
            let search = self.browser.feed()?.feed.search.clone()?;
            let form = Form::new(
                FormKind::Search(search),
                vec![Field::new(Msg::FieldQuery, "")],
            );
            return self.open_form(form);
        }
        let index = FEED.row_at(page, rows, y)?;
        let feed = &self.browser.feed()?.feed;
        let Some(entry) = feed.entries.get(index) else {
            // Son satır: sonraki sayfa.
            let next = feed.next.clone()?;
            return self.load(Request::Feed(next));
        };
        match &entry.kind {
            EntryKind::Navigation(url) => {
                let url = url.clone();
                self.load(Request::Feed(url))
            }
            EntryKind::Book(_) => {
                self.view = View::Entry { index };
                Some(self.refresh(true))
            }
            EntryKind::Unavailable => {
                self.view = View::Message {
                    msg: Msg::EntryUnavailable,
                    back: Back::Feed,
                };
                Some(self.refresh(true))
            }
        }
    }

    pub(super) fn draw_feed(&self, canvas: &mut Canvas) {
        let Some(f) = self.browser.feed() else {
            return;
        };
        let t = &self.text;
        let catalog = self
            .browser
            .catalogs
            .get(self.browser.current)
            .map_or(String::new(), |c| c.name.clone());
        let title = if f.feed.title.trim().is_empty() {
            catalog.clone()
        } else {
            f.feed.title.trim().to_string()
        };
        let subtitle = format!("{catalog} · {}", tr(&Msg::EntryCount(f.feed.entries.len())));
        let search = self.search_button().map(|_| Msg::Search);
        self.draw_browse_header(canvas, &title, &subtitle, search);
        if f.feed.entries.is_empty() {
            self.draw_message_lines(canvas, &Msg::FeedEmpty, HEADER_BOTTOM + 80);
        }
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
        for (index, row) in FEED.visible(f.page, self.browser.rows()) {
            let r = FEED.row_rect(canvas.width(), row);
            let Some(e) = f.feed.entries.get(index) else {
                t.draw_msg(canvas, &Msg::NextPage, MARGIN, r.y + 70, ROW_PX);
                continue;
            };
            let title = one_line(t, &e.title, BOLD, ROW_PX, max_w);
            t.draw_plain(canvas, &title, Family::Ui, BOLD, MARGIN, r.y + 50, ROW_PX);
            let second = match &e.kind {
                EntryKind::Unavailable => tr(&Msg::Unavailable),
                EntryKind::Book(_) if !e.authors.is_empty() => e.authors.join(", "),
                EntryKind::Book(_) => tr(&Msg::UnknownAuthor),
                EntryKind::Navigation(_) => e.summary.clone(),
            };
            let second = one_line(t, &second, PLAIN, 30.0, max_w);
            t.draw_plain(canvas, &second, Family::Ui, PLAIN, MARGIN, r.y + 95, 30.0);
        }
        let footer = Msg::ListFooter {
            page: f.page + 1,
            pages: FEED.pages(self.browser.rows()),
        };
        draw_footer(t, canvas, &footer, MARGIN);
    }

    // --- Kitap ayrıntısı ---

    /// Ayrıntı ekranındaki İndir / İptal / Yeniden dene / Aç düğmesi.
    pub(super) fn entry_button(&self) -> Rect {
        Rect {
            x: MARGIN,
            y: HEADER_BOTTOM + 30,
            w: self.width as i32 - 2 * MARGIN,
            h: 110,
        }
    }

    pub(super) fn handle_entry(&mut self, index: usize, event: Event) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        if self.entry_button().contains(x, y) {
            return self.entry_pressed(index);
        }
        if !back_button(self.width).contains(x, y) {
            return None;
        }
        self.view = View::Feed;
        Some(self.refresh(true))
    }

    pub(super) fn draw_entry(&self, canvas: &mut Canvas, index: usize) {
        let Some(e) = self.browser.feed().and_then(|f| f.feed.entries.get(index)) else {
            return;
        };
        let t = &self.text;
        let authors = if e.authors.is_empty() {
            tr(&Msg::UnknownAuthor)
        } else {
            e.authors.join(", ")
        };
        draw_header(t, canvas, &e.title, &authors, true);
        let button = self.entry_button();
        let mut y = button.y + button.h + 70;
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
        if let Some((_, label, error)) = self.entry_action(index) {
            draw_button(t, canvas, button, &tr(&label), ROW_PX);
            for line in error.map_or(Vec::new(), |m| {
                t.wrap_plain(&tr(&m), Family::Ui, BOLD, ROW_PX, max_w)
            }) {
                t.draw_plain(canvas, &line, Family::Ui, BOLD, MARGIN, y, ROW_PX);
                y += 52;
            }
            y += 20;
        }
        if let EntryKind::Book(links) = &e.kind {
            let formats: Vec<&str> = links
                .iter()
                .map(|(f, _)| match f {
                    Format::Epub => "EPUB",
                    Format::Pdf => "PDF",
                })
                .collect();
            t.draw_msg(canvas, &Msg::Formats(formats.join(", ")), MARGIN, y, ROW_PX);
            y += 80;
        }
        let bottom = crate::LIBRA2_HEIGHT as i32 - 120;
        for line in t.wrap_plain(&e.summary, Family::Ui, PLAIN, ROW_PX, max_w) {
            if y > bottom {
                break;
            }
            t.draw_plain(canvas, &line, Family::Ui, PLAIN, MARGIN, y, ROW_PX);
            y += 52;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_addresses_are_completed() {
        for (typed, full) in [
            ("192.168.1.5:8080", "http://192.168.1.5:8080/opds"),
            ("kitap.ev/katalog", "http://kitap.ev/katalog"),
            (
                "https://www.gutenberg.org/ebooks.opds/",
                "https://www.gutenberg.org/ebooks.opds/",
            ),
            ("https://ornek.org", "https://ornek.org/opds"),
            ("ornek.org?x=1", "http://ornek.org?x=1"),
        ] {
            assert_eq!(complete_url(typed), full);
        }
    }

    #[test]
    fn credentials_stay_on_the_catalog_server() {
        let c = Catalog {
            name: "Ev".into(),
            url: "https://Kitap.ornek.org:8080/opds".into(),
            auth: Some(Auth {
                user: "a".into(),
                password: "b".into(),
            }),
            members_only: false,
        };
        assert!(auth_for(&c, "https://kitap.ornek.org:8080/opds/yeni?s=2").is_some());
        assert!(
            auth_for(&c, "https://kitap.ornek.org/opds").is_none(),
            "port farklı"
        );
        assert!(
            auth_for(&c, "http://kitap.ornek.org:8080/opds").is_none(),
            "şema farklı"
        );
        assert!(auth_for(&c, "https://kitap.ornek.org:8080.kotu.com/").is_none());
        assert!(auth_for(&c, "https://baska.org/kitap.epub").is_none());
    }
}
