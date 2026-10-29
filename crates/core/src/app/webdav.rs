//! WebDAV (Nextcloud, ownCloud…): yedeği gönderme/alma ve klasörlerde gezip
//! kitap indirme. Hesap (adres, kullanıcı, şifre) Araçlar → WebDAV'dan girilir,
//! veri klasöründe `webdav.json`'da tutulur, yedeğe girmez.
//!
//! - Yedek, adresin gösterdiği klasöre "Legentia yedek.json" olarak (yerel
//!   yedekle aynı biçimde) yazılır; alınırken yerel kayıtlarla birleşir.
//! - Gezinme `PROPFIND` (Depth: 1) ile; yalnız klasörler ve kitaplar
//!   listelenir, kitaplar indirme kuyruğuyla `İndirilenler`'e iner.
//! - Şifre yalnızca sunucu 401 ile isteyince ve yalnızca hesabın kendi
//!   sunucusuna gönderilir (kataloglardaki gibi).

use quick_xml::events::Event as Xml;
use serde::{Deserialize, Serialize};

use super::backup::{BACKUP_FILE, Backup};
use super::form::{Field, Form, FormKind};
use super::jobs::Job;
use super::ui::{
    BOLD, HEADER_BOTTOM, List, MARGIN, PLAIN, ROW_PX, Rect, back_button, draw_button, draw_footer,
    draw_header, one_line,
};
use super::{App, View};
use crate::canvas::Canvas;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};
use crate::library::Format;
use crate::net::{Auth, Net};
use crate::text::Family;

pub(super) const WEBDAV_FILE: &str = "webdav.json";
const LISTING_LIMIT: u64 = 8 << 20;
const BACKUP_LIMIT: u64 = 64 << 20;
const BUTTON_H: i32 = 110;
pub(super) const DAV_LIST: List = List {
    top: HEADER_BOTTOM,
    row_h: 110,
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Account {
    /// Klasörün adresi (ör. https://bulut.ornek.org/remote.php/dav/files/ad/Kitaplar/).
    pub url: String,
    pub user: String,
    pub password: String,
}

impl Account {
    fn auth(&self) -> Option<Auth> {
        (!self.user.is_empty()).then(|| Auth {
            user: self.user.clone(),
            password: self.password.clone(),
        })
    }

    /// Sondaki `/` ile klasör adresi.
    fn dir(&self) -> String {
        let u = self.url.trim();
        if u.ends_with('/') {
            u.to_string()
        } else {
            format!("{u}/")
        }
    }
}

/// Klasördeki bir girdi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Entry {
    pub name: String,
    pub url: String,
    pub dir: bool,
}

/// Arka plan işinin sonucu.
pub(super) enum Done {
    Listing { url: String, entries: Vec<Entry> },
    Pushed,
    Pulled(Box<Backup>),
}

/// WebDAV durumu: hesap, sürmekte olan iş, açılan klasörler (sonuncusu gösterilir).
#[derive(Default)]
pub(super) struct Dav {
    pub account: Option<Account>,
    pub job: Option<Job<Result<Done, Msg>>>,
    pub dirs: Vec<(String, Vec<Entry>)>,
}

/// Şema ve sunucu adı (`https://ornek.org:8080`).
fn origin(url: &str) -> &str {
    let start = url.find("://").map_or(0, |i| i + 3);
    let end = url[start..].find('/').map_or(url.len(), |i| start + i);
    &url[..end]
}

/// Yol parçası için yüzde kodlama (harf, rakam ve `-._~` dışındakiler).
fn encode_segment(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn decode_percent(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `PROPFIND` yanıtındaki girdiler; istenen klasörün kendisi, başka sunucuya
/// gösterenler ve kitap olmayan dosyalar atılır. Klasörler önce, adlarına göre.
pub(super) fn parse_listing(xml: &str, dir_url: &str) -> Vec<Entry> {
    let base = origin(dir_url);
    let own_path = decode_percent(&dir_url[base.len()..]);
    let mut r = crate::epub::lenient_reader(xml);
    let mut out = Vec::new();
    let (mut href, mut name, mut is_dir) = (String::new(), String::new(), false);
    let mut field: Option<&'static str> = None;
    while let Ok(ev) = r.read_event() {
        match ev {
            Xml::Start(e) | Xml::Empty(e) => match e.local_name().as_ref() {
                "response" => {
                    (href, name, is_dir) = (String::new(), String::new(), false);
                }
                "href" => field = Some("href"),
                "displayname" => field = Some("name"),
                "collection" => is_dir = true,
                _ => {}
            },
            Xml::Text(t) => match field {
                Some("href") => href.push_str(t.xml10_content().trim()),
                Some("name") => name.push_str(&t.xml10_content()),
                _ => {}
            },
            Xml::GeneralRef(g) if field.is_some() => {
                let mut s = String::new();
                crate::epub::push_ref(&mut s, &g);
                if field == Some("href") {
                    href.push_str(&s);
                } else {
                    name.push_str(&s);
                }
            }
            Xml::End(e) => match e.local_name().as_ref() {
                "href" | "displayname" => field = None,
                "response" => {
                    // Başka sunucuya gösteren tam adres alınmaz.
                    let path = match href.strip_prefix(base) {
                        Some(p) => p.to_string(),
                        None if href.starts_with('/') => href.clone(),
                        None => continue,
                    };
                    let decoded = decode_percent(&path);
                    if decoded.trim_end_matches('/') == own_path.trim_end_matches('/') {
                        continue;
                    }
                    let name = if name.trim().is_empty() {
                        decoded
                            .trim_end_matches('/')
                            .rsplit('/')
                            .next()
                            .unwrap_or_default()
                            .to_string()
                    } else {
                        name.trim().to_string()
                    };
                    let book = Format::of(std::path::Path::new(&name)).is_some();
                    if !name.is_empty() && !name.starts_with('.') && (is_dir || book) {
                        out.push(Entry {
                            name,
                            url: format!("{base}{path}"),
                            dir: is_dir,
                        });
                    }
                }
                _ => {}
            },
            Xml::Eof => break,
            _ => {}
        }
    }
    out.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then_with(|| crate::collate::compare(&a.name, &b.name))
    });
    out
}

fn status_error(status: u16) -> Msg {
    match status {
        401 | 403 => Msg::NetUnauthorized,
        code => Msg::NetStatus(code),
    }
}

const PROPFIND: &str = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/><d:displayname/></d:prop></d:propfind>"#;

/// Ayrı iş parçacığında: klasör listesi.
fn list(net: &Net, account: &Account, url: &str) -> Result<Done, Msg> {
    let headers = [
        ("Depth", "1".to_string()),
        ("Content-Type", "application/xml; charset=utf-8".to_string()),
    ];
    let (status, body) = net
        .send_auth(
            "PROPFIND",
            url,
            &headers,
            PROPFIND.as_bytes().to_vec(),
            LISTING_LIMIT,
            account.auth().as_ref(),
        )
        .map_err(|e| e.message())?;
    if !(200..300).contains(&status) {
        return Err(status_error(status));
    }
    let entries = parse_listing(&String::from_utf8_lossy(&body), url);
    Ok(Done::Listing {
        url: url.to_string(),
        entries,
    })
}

fn backup_url(account: &Account) -> String {
    format!("{}{}", account.dir(), encode_segment(BACKUP_FILE))
}

fn push(net: &Net, account: &Account, data: Vec<u8>) -> Result<Done, Msg> {
    let headers = [("Content-Type", "application/json".to_string())];
    let (status, _) = net
        .send_auth(
            "PUT",
            &backup_url(account),
            &headers,
            data,
            1 << 16,
            account.auth().as_ref(),
        )
        .map_err(|e| e.message())?;
    if !(200..300).contains(&status) {
        return Err(status_error(status));
    }
    Ok(Done::Pushed)
}

fn pull(net: &Net, account: &Account) -> Result<Done, Msg> {
    let (status, body) = net
        .send_auth(
            "GET",
            &backup_url(account),
            &[],
            Vec::new(),
            BACKUP_LIMIT,
            account.auth().as_ref(),
        )
        .map_err(|e| e.message())?;
    match status {
        200..=299 => crate::storage::decode::<Backup>(&body)
            .map(|b| Done::Pulled(Box::new(b)))
            .ok_or(Msg::BackupFailed),
        404 => Err(Msg::DavNoBackup),
        s => Err(status_error(s)),
    }
}

impl App {
    /// WebDAV ekranının düğmeleri: yedeği gönder, yedeği al, gez, hesap.
    pub(super) fn dav_buttons(&self) -> [Rect; 4] {
        let r = |i: i32| Rect {
            x: MARGIN,
            y: HEADER_BOTTOM + 200 + i * (BUTTON_H + 40),
            w: self.width as i32 - 2 * MARGIN,
            h: BUTTON_H,
        };
        [r(0), r(1), r(2), r(3)]
    }

    pub(super) fn webdav_button(&self) -> Rect {
        self.tool_row(5)
    }

    pub(super) fn open_webdav(&mut self) -> Option<Refresh> {
        if !self.settings.network {
            self.show_message(Msg::NetworkOff);
            return Some(self.refresh(true));
        }
        if self.dav.account.is_none() {
            return self.open_dav_form();
        }
        self.view = View::WebDav;
        Some(self.refresh(true))
    }

    fn open_dav_form(&mut self) -> Option<Refresh> {
        let a = self.dav.account.clone().unwrap_or_default();
        let mut password = Field::new(Msg::FieldPassword, &a.password);
        password.secret = true;
        let fields = vec![
            Field::new(Msg::FieldDavUrl, &a.url),
            Field::new(Msg::FieldUser, &a.user),
            password,
        ];
        self.open_form(Form::new(FormKind::WebDav, fields))
    }

    /// Hesap formu: adres boşsa hesap silinir.
    pub(super) fn save_dav_account(&mut self, form: &Form) -> Result<(), Msg> {
        let value = |i: usize| form.fields[i].value.trim().to_string();
        let url = value(0);
        let file = self.store.dir().join(WEBDAV_FILE);
        if url.is_empty() {
            self.dav.account = None;
            let _ = std::fs::remove_file(file);
            return Ok(());
        }
        if !(url.starts_with("https://") || url.starts_with("http://")) || url.contains(' ') {
            return Err(Msg::InvalidUrl);
        }
        let account = Account {
            url,
            user: value(1),
            password: form.fields[2].value.clone(),
        };
        self.store.save(WEBDAV_FILE, &account);
        self.dav.account = Some(account);
        self.dav.dirs.clear();
        Ok(())
    }

    fn start_dav(
        &mut self,
        work: impl FnOnce(&Net, &Account) -> Result<Done, Msg> + Send + 'static,
    ) {
        let Some(account) = self.dav.account.clone() else {
            return;
        };
        let net = self.browser.net();
        self.dav.job = Some(Job::spawn(move |_| work(&net, &account)));
    }

    /// `Event::Tick`: WebDAV işi bittiyse sonucu.
    pub(super) fn poll_dav(&mut self) -> Option<Refresh> {
        let result = self.dav.job.as_ref()?.poll()?;
        self.dav.job = None;
        let result = result.unwrap_or(Err(Msg::NetInvalid));
        let back = super::Back::Library;
        match result {
            Ok(Done::Listing { url, entries }) => {
                self.dav.dirs.push((url, entries));
                self.view = View::DavBrowse { page: 0 };
            }
            Ok(Done::Pushed) => {
                self.view = View::Message {
                    msg: Msg::DavPushed,
                    back,
                }
            }
            Ok(Done::Pulled(backup)) => {
                let msg = self.restore_from(*backup);
                self.view = View::Message { msg, back };
            }
            Err(msg) => self.view = View::Message { msg, back },
        }
        Some(self.refresh(true))
    }

    pub(super) fn handle_webdav(&mut self, event: Event) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        if self.dav.job.is_some() {
            return None;
        }
        if back_button(self.width).contains(x, y) {
            self.view = View::Backup;
            return Some(self.refresh(true));
        }
        let [push_r, pull_r, browse_r, account_r] = self.dav_buttons();
        if push_r.contains(x, y) {
            let data = crate::storage::encode(&self.collect_backup());
            self.start_dav(move |net, a| push(net, a, data));
        } else if pull_r.contains(x, y) {
            self.start_dav(pull);
        } else if browse_r.contains(x, y) {
            self.dav.dirs.clear();
            self.start_dav(|net, a| list(net, a, &a.dir()));
        } else if account_r.contains(x, y) {
            return self.open_dav_form();
        } else {
            return None;
        }
        Some(self.refresh(false))
    }

    pub(super) fn draw_webdav(&self, canvas: &mut Canvas) {
        let t = &self.text;
        let url = self
            .dav
            .account
            .as_ref()
            .map(|a| a.url.clone())
            .unwrap_or_default();
        draw_header(t, canvas, "WebDAV", &url, true);
        draw_button(
            t,
            canvas,
            back_button(canvas.width()),
            &tr(&Msg::Back),
            ROW_PX,
        );
        if self.dav.job.is_some() {
            t.draw_msg(canvas, &Msg::Loading, MARGIN, HEADER_BOTTOM + 100, ROW_PX);
            return;
        }
        let labels = [Msg::DavPush, Msg::DavPull, Msg::DavBrowse, Msg::DavAccount];
        for (r, label) in self.dav_buttons().iter().zip(labels) {
            draw_button(t, canvas, *r, &tr(&label), ROW_PX);
        }
    }

    pub(super) fn handle_dav_browse(&mut self, page: usize, event: Event) -> Option<Refresh> {
        if self.dav.job.is_some() {
            return None;
        }
        if let Event::Tap { x, y } = event {
            if back_button(self.width).contains(x, y) {
                self.dav.dirs.pop();
                self.view = if self.dav.dirs.is_empty() {
                    View::WebDav
                } else {
                    View::DavBrowse { page: 0 }
                };
                return Some(self.refresh(true));
            }
            let entries = &self.dav.dirs.last()?.1;
            let row = DAV_LIST.row_at(page, entries.len(), y)?;
            let e = entries[row].clone();
            if e.dir {
                self.start_dav(move |net, a| list(net, a, &e.url));
                return Some(self.refresh(false));
            }
            let dest = self
                .root
                .join(super::downloads::DOWNLOAD_DIR)
                .join(e.name.replace(['/', '\\'], " "));
            let auth = self.dav.account.as_ref().and_then(Account::auth);
            self.enqueue_download(e.name.clone(), e.url.clone(), auth, dest);
            return Some(self.refresh(false));
        }
        let len = self.dav.dirs.last().map_or(0, |d| d.1.len());
        let page = Self::page_list(page, DAV_LIST.pages(len), event)?;
        self.view = View::DavBrowse { page };
        Some(self.refresh(false))
    }

    pub(super) fn draw_dav_browse(&self, canvas: &mut Canvas, page: usize) {
        let t = &self.text;
        let Some((url, entries)) = self.dav.dirs.last() else {
            return;
        };
        let title = decode_percent(
            url.trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or_default(),
        );
        draw_header(
            t,
            canvas,
            &title,
            &tr(&Msg::EntryCount(entries.len())),
            true,
        );
        draw_button(
            t,
            canvas,
            back_button(canvas.width()),
            &tr(&Msg::Back),
            ROW_PX,
        );
        if self.dav.job.is_some() {
            t.draw_msg(canvas, &Msg::Loading, MARGIN, HEADER_BOTTOM + 80, ROW_PX);
            return;
        }
        if entries.is_empty() {
            self.draw_message_lines(canvas, &Msg::DavEmpty, HEADER_BOTTOM + 80);
        }
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
        for (index, row) in DAV_LIST.visible(page, entries.len()) {
            let e = &entries[index];
            let r = DAV_LIST.row_rect(canvas.width(), row);
            let (style, name) = if e.dir {
                (BOLD, format!("{}/", e.name))
            } else {
                (PLAIN, e.name.clone())
            };
            let mut line = one_line(t, &name, style, ROW_PX, max_w);
            if !e.dir
                && let Some(state) = self.download_note(&e.name)
            {
                line = one_line(t, &format!("{name} · {state}"), style, ROW_PX, max_w);
            }
            t.draw_plain(canvas, &line, Family::Ui, style, MARGIN, r.y + 65, ROW_PX);
        }
        let footer = Msg::ListFooter {
            page: page + 1,
            pages: DAV_LIST.pages(entries.len()),
        };
        draw_footer(t, canvas, &footer, MARGIN);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listings_keep_folders_and_books_on_the_same_server() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
 <d:response><d:href>/dav/Kitaplar/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response>
 <d:response><d:href>/dav/Kitaplar/Roman%20lar/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response>
 <d:response><d:href>/dav/Kitaplar/%C3%87al%C4%B1ku%C5%9Fu.epub</d:href><d:propstat><d:prop><d:resourcetype/></d:prop></d:propstat></d:response>
 <d:response><d:href>https://ornek.org/dav/Kitaplar/b.pdf</d:href><d:propstat><d:prop><d:displayname>Belge.pdf</d:displayname><d:resourcetype/></d:prop></d:propstat></d:response>
 <d:response><d:href>/dav/Kitaplar/resim.jpg</d:href><d:propstat><d:prop><d:resourcetype/></d:prop></d:propstat></d:response>
 <d:response><d:href>https://baska.org/x.epub</d:href><d:propstat><d:prop><d:resourcetype/></d:prop></d:propstat></d:response>
</d:multistatus>"#;
        let e = parse_listing(xml, "https://ornek.org/dav/Kitaplar/");
        let names: Vec<(&str, bool)> = e.iter().map(|e| (e.name.as_str(), e.dir)).collect();
        assert_eq!(
            names,
            [
                ("Roman lar", true),
                ("Belge.pdf", false),
                ("Çalıkuşu.epub", false)
            ]
        );
        assert_eq!(
            e[2].url,
            "https://ornek.org/dav/Kitaplar/%C3%87al%C4%B1ku%C5%9Fu.epub"
        );
        assert_eq!(
            encode_segment("Legentia yedek.json"),
            "Legentia%20yedek.json"
        );
    }
}
