//! Dropbox: yedeği gönderme/alma ve klasörlerde gezip kitap indirme
//! (WebDAV ekranıyla aynı işler). Uygulama anahtarı Legentia'nın kendi Dropbox
//! uygulamasınındır; PKCE kullanıldığı için gizli değildir.
//!
//! Bağlanma: Dropbox'ın yönlendirmesiz PKCE akışında kullanıcı uzun bir
//! adresi açıp dönen kodu uygulamaya yazmalıdır. E Ink klavyede bu zor
//! olduğundan cihaz, kablosuz yüklemedeki gibi kısa ömürlü küçük bir yerel
//! sayfa açar: telefonda/bilgisayarda ekrandaki adres ve 6 haneli kodla
//! girilince sayfa Dropbox'a yönlendirir, Dropbox'ın verdiği kod aynı sayfaya
//! yapıştırılınca cihaza ulaşır. Kod, aynı ağdaki başkasının cihazı kendi
//! Dropbox'una bağlamasını önler. Şifre cihaza girmez; yalnızca yenileme
//! belirteci `dropbox.json`'da tutulur (yedeğe girmez).

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::backup::{BACKUP_FILE, Backup};
use super::jobs::Job;
use super::ui::{
    BOLD, HEADER_BOTTOM, List, MARGIN, PLAIN, ROW_PX, Rect, back_button, draw_button, draw_footer,
    draw_header, one_line,
};
use super::upload::{local_ip, new_code, query_value, respond};
use super::{App, View};
use crate::canvas::Canvas;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};
use crate::library::Format;
use crate::net::Net;
use crate::text::Family;

pub(super) const DROPBOX_FILE: &str = "dropbox.json";
/// Legentia'nın Dropbox uygulama anahtarı (App Console).
const APP_KEY: &str = "0gzqlf7pa2k80lv";
const FIRST_PORT: u16 = 8090;
pub(super) const DBX_LIST: List = List {
    top: HEADER_BOTTOM,
    row_h: 110,
};

/// Dropbox adresleri (testlerde yerel sunucu).
#[derive(Debug, Clone)]
pub(super) struct Hosts {
    pub auth: String,
    pub api: String,
    pub content: String,
}

impl Default for Hosts {
    fn default() -> Self {
        Hosts {
            auth: "https://www.dropbox.com".into(),
            api: "https://api.dropboxapi.com".into(),
            content: "https://content.dropboxapi.com".into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Account {
    pub refresh_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Entry {
    pub name: String,
    pub path: String,
    pub dir: bool,
}

pub(super) enum Done {
    Connected(Account),
    Listing {
        path: String,
        entries: Vec<Entry>,
    },
    Pushed,
    Pulled(Box<Backup>),
    /// İndirilecek dosyanın geçici bağlantısı (ad, adres).
    Link(String, String),
}

/// Bağlanma sayfasının sunucusu.
pub(super) struct Connect {
    pub address: String,
    pub code: String,
    verifier: String,
    stop: Arc<AtomicBool>,
    rx: Receiver<String>,
}

impl Drop for Connect {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub(super) struct Dropbox {
    pub account: Option<Account>,
    pub hosts: Hosts,
    pub connect: Option<Connect>,
    pub job: Option<Job<Result<Done, Msg>>>,
    pub dirs: Vec<(String, Vec<Entry>)>,
}

/// URL'de güvenli base64 (dolgusuz).
fn base64url(data: &[u8]) -> String {
    crate::net::base64(data)
        .trim_end_matches('=')
        .replace('+', "-")
        .replace('/', "_")
}

/// PKCE doğrulayıcısı: 32 rastgele bayttan 43 karakter.
fn new_verifier() -> String {
    use std::io::Read;
    let mut bytes = [0u8; 32];
    let ok = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_ok();
    if !ok {
        // /dev/urandom yoksa (olmamalı) saat ve süreçten karıştırılır.
        let mut x = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos() as u64)
            ^ std::process::id() as u64;
        for b in &mut bytes {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = x as u8;
        }
    }
    base64url(&bytes)
}

fn challenge(verifier: &str) -> String {
    base64url(&Sha256::digest(verifier.as_bytes()))
}

fn authorize_url(hosts: &Hosts, verifier: &str) -> String {
    format!(
        "{}/oauth2/authorize?client_id={APP_KEY}&response_type=code&code_challenge={}&code_challenge_method=S256&token_access_type=offline",
        hosts.auth,
        challenge(verifier)
    )
}

/// HTTP başlığında JSON (Dropbox-API-Arg): ASCII dışı karakterler \u ile.
fn header_json(v: &Value) -> String {
    let mut out = String::new();
    for c in v.to_string().chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut buf = [0u16; 2];
            for u in c.encode_utf16(&mut buf) {
                out.push_str(&format!("\\u{u:04x}"));
            }
        }
    }
    out
}

fn form(pairs: &[(&str, &str)]) -> Vec<u8> {
    pairs
        .iter()
        .map(|(k, v)| {
            let enc: String = v
                .bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                        (b as char).to_string()
                    }
                    _ => format!("%{b:02X}"),
                })
                .collect();
            format!("{k}={enc}")
        })
        .collect::<Vec<_>>()
        .join("&")
        .into_bytes()
}

fn token(net: &Net, hosts: &Hosts, pairs: &[(&str, &str)]) -> Result<Value, Msg> {
    let (status, body) = net
        .send(
            "POST",
            &format!("{}/oauth2/token", hosts.api),
            &[("Content-Type", "application/x-www-form-urlencoded".into())],
            form(pairs),
            1 << 16,
        )
        .map_err(|e| e.message())?;
    if !(200..300).contains(&status) {
        log::warn!("dropbox token: {status} {}", String::from_utf8_lossy(&body));
        return Err(Msg::DropboxAuth);
    }
    serde_json::from_slice(&body).map_err(|_| Msg::NetInvalid)
}

fn exchange(net: &Net, hosts: &Hosts, code: &str, verifier: &str) -> Result<Done, Msg> {
    let v = token(
        net,
        hosts,
        &[
            ("code", code.trim()),
            ("grant_type", "authorization_code"),
            ("code_verifier", verifier),
            ("client_id", APP_KEY),
        ],
    )?;
    let refresh = v
        .get("refresh_token")
        .and_then(Value::as_str)
        .ok_or(Msg::DropboxAuth)?;
    Ok(Done::Connected(Account {
        refresh_token: refresh.to_string(),
    }))
}

fn access(net: &Net, hosts: &Hosts, account: &Account) -> Result<String, Msg> {
    let v = token(
        net,
        hosts,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &account.refresh_token),
            ("client_id", APP_KEY),
        ],
    )?;
    v.get("access_token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or(Msg::DropboxAuth)
}

/// API çağrısı; Dropbox hata gövdesinden anlaşılır mesaj.
fn call(
    net: &Net,
    url: &str,
    access: &str,
    headers: &[(&str, String)],
    body: Vec<u8>,
    limit: u64,
) -> Result<(u16, Vec<u8>), Msg> {
    let mut all = vec![("Authorization", format!("Bearer {access}"))];
    all.extend(headers.iter().map(|(k, v)| (*k, v.clone())));
    let (status, out) = net
        .send("POST", url, &all, body, limit)
        .map_err(|e| e.message())?;
    if status == 401 || status == 400 && String::from_utf8_lossy(&out).contains("scope") {
        return Err(Msg::DropboxScope);
    }
    Ok((status, out))
}

fn json_headers() -> Vec<(&'static str, String)> {
    vec![("Content-Type", "application/json".into())]
}

fn list(net: &Net, hosts: &Hosts, account: &Account, path: &str) -> Result<Done, Msg> {
    let token = access(net, hosts, account)?;
    let mut entries = Vec::new();
    let mut body = json!({ "path": path }).to_string();
    let mut url = format!("{}/2/files/list_folder", hosts.api);
    loop {
        let (status, out) = call(
            net,
            &url,
            &token,
            &json_headers(),
            body.into_bytes(),
            8 << 20,
        )?;
        if !(200..300).contains(&status) {
            return Err(Msg::NetStatus(status));
        }
        let v: Value = serde_json::from_slice(&out).map_err(|_| Msg::NetInvalid)?;
        for e in v
            .get("entries")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let name = e.get("name").and_then(Value::as_str).unwrap_or_default();
            let path = e
                .get("path_display")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let dir = e.get(".tag").and_then(Value::as_str) == Some("folder");
            let book = Format::of(std::path::Path::new(name)).is_some();
            if !name.starts_with('.') && (dir || book) {
                entries.push(Entry {
                    name: name.to_string(),
                    path: path.to_string(),
                    dir,
                });
            }
        }
        let cursor = v.get("cursor").and_then(Value::as_str).unwrap_or_default();
        if v.get("has_more").and_then(Value::as_bool) != Some(true) || entries.len() > 5000 {
            break;
        }
        body = json!({ "cursor": cursor }).to_string();
        url = format!("{}/2/files/list_folder/continue", hosts.api);
    }
    entries.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then_with(|| crate::collate::compare(&a.name, &b.name))
    });
    Ok(Done::Listing {
        path: path.to_string(),
        entries,
    })
}

fn push(net: &Net, hosts: &Hosts, account: &Account, data: Vec<u8>) -> Result<Done, Msg> {
    let token = access(net, hosts, account)?;
    let arg = json!({ "path": format!("/{BACKUP_FILE}"), "mode": "overwrite", "mute": true });
    let headers = [
        ("Content-Type", "application/octet-stream".to_string()),
        ("Dropbox-API-Arg", header_json(&arg)),
    ];
    let url = format!("{}/2/files/upload", hosts.content);
    let (status, _) = call(net, &url, &token, &headers, data, 1 << 16)?;
    if !(200..300).contains(&status) {
        return Err(Msg::NetStatus(status));
    }
    Ok(Done::Pushed)
}

fn pull(net: &Net, hosts: &Hosts, account: &Account) -> Result<Done, Msg> {
    let token = access(net, hosts, account)?;
    let arg = json!({ "path": format!("/{BACKUP_FILE}") });
    let headers = [("Dropbox-API-Arg", header_json(&arg))];
    let url = format!("{}/2/files/download", hosts.content);
    let (status, body) = call(net, &url, &token, &headers, Vec::new(), 64 << 20)?;
    match status {
        200..=299 => crate::storage::decode::<Backup>(&body)
            .map(|b| Done::Pulled(Box::new(b)))
            .ok_or(Msg::BackupFailed),
        409 => Err(Msg::DavNoBackup),
        s => Err(Msg::NetStatus(s)),
    }
}

/// İndirme kuyruğu GET ister: Dropbox'tan dört saatlik geçici bağlantı alınır.
fn link(net: &Net, hosts: &Hosts, account: &Account, entry: &Entry) -> Result<Done, Msg> {
    let token = access(net, hosts, account)?;
    let url = format!("{}/2/files/get_temporary_link", hosts.api);
    let body = json!({ "path": entry.path }).to_string().into_bytes();
    let (status, out) = call(net, &url, &token, &json_headers(), body, 1 << 16)?;
    if !(200..300).contains(&status) {
        return Err(Msg::NetStatus(status));
    }
    let v: Value = serde_json::from_slice(&out).map_err(|_| Msg::NetInvalid)?;
    let link = v
        .get("link")
        .and_then(Value::as_str)
        .ok_or(Msg::NetInvalid)?;
    Ok(Done::Link(entry.name.clone(), link.to_string()))
}

/// Bağlanma sayfası (6 haneli kodla korunur).
fn page(body: &str) -> String {
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Legentia</title>
<style>body{{font-family:sans-serif;max-width:40em;margin:2em auto;padding:0 1em}}input{{font-size:1.2em}}</style></head><body><h1>Legentia · Dropbox</h1>{body}</body></html>"#
    )
}

fn connect_server(
    listener: TcpListener,
    code: String,
    authorize: String,
    stop: Arc<AtomicBool>,
    tx: std::sync::mpsc::Sender<String>,
) {
    let html = |s: &str| s.replace('<', "&lt;");
    while !stop.load(Ordering::Relaxed) {
        let mut stream = match listener.accept() {
            Ok((s, _)) => s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(_) => break,
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let mut first = String::new();
        let Ok(clone) = stream.try_clone() else {
            continue;
        };
        let mut reader = BufReader::new(clone);
        if reader.read_line(&mut first).is_err() {
            continue;
        }
        // Başlıkların geri kalanı okunup atılır.
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).map_or(true, |n| n == 0) || line.trim().is_empty() {
                break;
            }
        }
        let target = first.split_whitespace().nth(1).unwrap_or("/");
        let query = target.split_once('?').map_or("", |(_, q)| q);
        let given = query_value(query, "kod").unwrap_or_default();
        let ask_code = format!(
            "<form><p>{}: <input name=\"kod\" inputmode=\"numeric\" size=\"6\"> <button>OK</button></p></form>",
            html(&tr(&Msg::UploadCode))
        );
        let body = if given != code {
            let wrong = if given.is_empty() {
                String::new()
            } else {
                format!("<p><b>{}</b></p>", html(&tr(&Msg::UploadWrongCode)))
            };
            format!(
                "<p>{}</p>{wrong}{ask_code}",
                html(&tr(&Msg::DropboxPageCode))
            )
        } else if let Some(dbx) = query_value(query, "dbx").filter(|d| !d.trim().is_empty()) {
            let _ = tx.send(dbx);
            format!("<p>{}</p>", html(&tr(&Msg::DropboxPageDone)))
        } else {
            format!(
                "<p>1. <a href=\"{}\" target=\"_blank\">{}</a></p><form><input type=\"hidden\" name=\"kod\" value=\"{code}\"><p>2. {} <input name=\"dbx\" size=\"50\"> <button>OK</button></p></form>",
                authorize.replace('"', "&quot;"),
                html(&tr(&Msg::DropboxPageOpen)),
                html(&tr(&Msg::DropboxPagePaste))
            )
        };
        respond(&mut stream, "200 OK", "text/html", &page(&body));
    }
}

impl App {
    pub(super) fn dropbox_button(&self) -> Rect {
        self.service_row(4)
    }

    pub(super) fn open_dropbox(&mut self) -> Option<Refresh> {
        if !self.settings.network {
            self.show_message(Msg::NetworkOff);
            return Some(self.refresh(true));
        }
        if self.dropbox.account.is_some() {
            self.view = View::Dropbox;
            return Some(self.refresh(true));
        }
        self.start_dropbox_connect(local_ip())
    }

    /// Bağlanma sayfasını açar; `ip` ekranda gösterilecek adres.
    pub(super) fn start_dropbox_connect(
        &mut self,
        ip: Option<std::net::IpAddr>,
    ) -> Option<Refresh> {
        let Some(ip) = ip else {
            self.show_message(Msg::UploadNoNetwork);
            return Some(self.refresh(true));
        };
        let Some(listener) =
            (FIRST_PORT..FIRST_PORT + 20).find_map(|p| TcpListener::bind(("0.0.0.0", p)).ok())
        else {
            self.show_message(Msg::UploadNoNetwork);
            return Some(self.refresh(true));
        };
        let port = listener.local_addr().ok()?.port();
        listener.set_nonblocking(true).ok()?;
        let (verifier, code) = (new_verifier(), new_code());
        let authorize = authorize_url(&self.dropbox.hosts, &verifier);
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let (c, s) = (code.clone(), stop.clone());
        std::thread::spawn(move || connect_server(listener, c, authorize, s, tx));
        self.dropbox.connect = Some(Connect {
            address: format!("http://{ip}:{port}"),
            code,
            verifier,
            stop,
            rx,
        });
        self.view = View::DropboxConnect;
        Some(self.refresh(true))
    }

    fn start_dropbox(
        &mut self,
        work: impl FnOnce(&Net, &Hosts) -> Result<Done, Msg> + Send + 'static,
    ) {
        let net = self.browser.net();
        let hosts = self.dropbox.hosts.clone();
        self.dropbox.job = Some(Job::spawn(move |_| work(&net, &hosts)));
    }

    /// `Event::Tick`: bağlanma sayfasından gelen kod ya da biten iş.
    pub(super) fn poll_dropbox(&mut self) -> Option<Refresh> {
        if let Some(c) = &self.dropbox.connect
            && let Ok(code) = c.rx.try_recv()
        {
            let verifier = c.verifier.clone();
            self.dropbox.connect = None;
            self.start_dropbox(move |net, hosts| exchange(net, hosts, &code, &verifier));
            self.view = View::Working;
            return Some(self.refresh(true));
        }
        let result = self.dropbox.job.as_ref()?.poll()?;
        self.dropbox.job = None;
        let back = super::Back::Library;
        match result.unwrap_or(Err(Msg::NetInvalid)) {
            Ok(Done::Connected(account)) => {
                self.store.save(DROPBOX_FILE, &account);
                self.dropbox.account = Some(account);
                self.view = View::Dropbox;
            }
            Ok(Done::Listing { path, entries }) => {
                self.dropbox.dirs.push((path, entries));
                self.view = View::DropboxBrowse { page: 0 };
            }
            Ok(Done::Pushed) => {
                self.view = View::Message {
                    msg: Msg::DropboxPushed,
                    back,
                }
            }
            Ok(Done::Pulled(b)) => {
                let msg = self.restore_from(*b);
                self.view = View::Message { msg, back };
            }
            Ok(Done::Link(name, url)) => {
                let dest = self
                    .root
                    .join(super::downloads::DOWNLOAD_DIR)
                    .join(name.replace(['/', '\\'], " "));
                self.enqueue_download(name, url, None, dest);
            }
            Err(msg) => self.view = View::Message { msg, back },
        }
        Some(self.refresh(true))
    }

    pub(super) fn handle_dropbox_connect(&mut self, event: Event) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        if back_button(self.width).contains(x, y) {
            self.dropbox.connect = None;
            self.view = View::Services;
            return Some(self.refresh(true));
        }
        None
    }

    pub(super) fn draw_dropbox_connect(&self, canvas: &mut Canvas) {
        let t = &self.text;
        draw_header(t, canvas, "Dropbox", "", true);
        draw_button(
            t,
            canvas,
            back_button(canvas.width()),
            &tr(&Msg::Cancel),
            ROW_PX,
        );
        let Some(c) = &self.dropbox.connect else {
            return;
        };
        let mut y = HEADER_BOTTOM + 60;
        self.draw_message_lines(canvas, &Msg::DropboxConnectHint, y);
        y += 300;
        t.draw_plain(canvas, &c.address, Family::Ui, BOLD, MARGIN, y, 52.0);
        y += 110;
        t.draw_msg(
            canvas,
            &Msg::UploadCodeLine(c.code.clone()),
            MARGIN,
            y,
            52.0,
        );
    }

    /// Bağlı Dropbox ekranının düğmeleri: gönder, al, gez, bağlantıyı kes.
    pub(super) fn dropbox_buttons(&self) -> [Rect; 4] {
        let r = |i: i32| Rect {
            x: MARGIN,
            y: HEADER_BOTTOM + 200 + i * 150,
            w: self.width as i32 - 2 * MARGIN,
            h: 110,
        };
        [r(0), r(1), r(2), r(3)]
    }

    pub(super) fn handle_dropbox(&mut self, event: Event) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        if self.dropbox.job.is_some() {
            return None;
        }
        if back_button(self.width).contains(x, y) {
            self.view = View::Services;
            return Some(self.refresh(true));
        }
        let account = self.dropbox.account.clone()?;
        let [push_r, pull_r, browse_r, logout_r] = self.dropbox_buttons();
        if push_r.contains(x, y) {
            let data = crate::storage::encode(&self.collect_backup());
            self.start_dropbox(move |net, h| push(net, h, &account, data));
        } else if pull_r.contains(x, y) {
            self.start_dropbox(move |net, h| pull(net, h, &account));
        } else if browse_r.contains(x, y) {
            self.dropbox.dirs.clear();
            self.start_dropbox(move |net, h| list(net, h, &account, ""));
        } else if logout_r.contains(x, y) {
            self.dropbox.account = None;
            let _ = std::fs::remove_file(self.store.dir().join(DROPBOX_FILE));
            self.view = View::Services;
            return Some(self.refresh(true));
        } else {
            return None;
        }
        Some(self.refresh(false))
    }

    pub(super) fn draw_dropbox(&self, canvas: &mut Canvas) {
        let t = &self.text;
        draw_header(t, canvas, "Dropbox", &tr(&Msg::DropboxConnected), true);
        draw_button(
            t,
            canvas,
            back_button(canvas.width()),
            &tr(&Msg::Back),
            ROW_PX,
        );
        if self.dropbox.job.is_some() {
            t.draw_msg(canvas, &Msg::Loading, MARGIN, HEADER_BOTTOM + 100, ROW_PX);
            return;
        }
        let labels = [
            Msg::DropboxPush,
            Msg::DropboxPull,
            Msg::DavBrowse,
            Msg::DropboxLogout,
        ];
        for (r, label) in self.dropbox_buttons().iter().zip(labels) {
            draw_button(t, canvas, *r, &tr(&label), ROW_PX);
        }
    }

    pub(super) fn handle_dropbox_browse(&mut self, page: usize, event: Event) -> Option<Refresh> {
        if self.dropbox.job.is_some() {
            return None;
        }
        if let Event::Tap { x, y } = event {
            if back_button(self.width).contains(x, y) {
                self.dropbox.dirs.pop();
                self.view = if self.dropbox.dirs.is_empty() {
                    View::Dropbox
                } else {
                    View::DropboxBrowse { page: 0 }
                };
                return Some(self.refresh(true));
            }
            let entries = &self.dropbox.dirs.last()?.1;
            let row = DBX_LIST.row_at(page, entries.len(), y)?;
            let e = entries[row].clone();
            let account = self.dropbox.account.clone()?;
            if e.dir {
                self.start_dropbox(move |net, h| list(net, h, &account, &e.path));
            } else {
                self.start_dropbox(move |net, h| link(net, h, &account, &e));
            }
            return Some(self.refresh(false));
        }
        let len = self.dropbox.dirs.last().map_or(0, |d| d.1.len());
        let page = Self::page_list(page, DBX_LIST.pages(len), event)?;
        self.view = View::DropboxBrowse { page };
        Some(self.refresh(false))
    }

    pub(super) fn draw_dropbox_browse(&self, canvas: &mut Canvas, page: usize) {
        let t = &self.text;
        let Some((path, entries)) = self.dropbox.dirs.last() else {
            return;
        };
        let title = if path.is_empty() {
            "Dropbox"
        } else {
            path.rsplit('/').next().unwrap_or(path)
        };
        draw_header(t, canvas, title, &tr(&Msg::EntryCount(entries.len())), true);
        draw_button(
            t,
            canvas,
            back_button(canvas.width()),
            &tr(&Msg::Back),
            ROW_PX,
        );
        if self.dropbox.job.is_some() {
            t.draw_msg(canvas, &Msg::Loading, MARGIN, HEADER_BOTTOM + 80, ROW_PX);
            return;
        }
        if entries.is_empty() {
            self.draw_message_lines(canvas, &Msg::DavEmpty, HEADER_BOTTOM + 80);
        }
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
        for (index, row) in DBX_LIST.visible(page, entries.len()) {
            let e = &entries[index];
            let r = DBX_LIST.row_rect(canvas.width(), row);
            let (style, name) = if e.dir {
                (BOLD, format!("{}/", e.name))
            } else {
                (PLAIN, e.name.clone())
            };
            let shown = match self.download_note(&e.name).filter(|_| !e.dir) {
                Some(state) => format!("{name} · {state}"),
                None => name,
            };
            let line = one_line(t, &shown, style, ROW_PX, max_w);
            t.draw_plain(canvas, &line, Family::Ui, style, MARGIN, r.y + 65, ROW_PX);
        }
        let footer = Msg::ListFooter {
            page: page + 1,
            pages: DBX_LIST.pages(entries.len()),
        };
        draw_footer(t, canvas, &footer, MARGIN);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_and_header_json_follow_the_spec() {
        // RFC 7636 ek B örneği.
        let v = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(challenge(v), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let fresh = new_verifier();
        assert_eq!(fresh.len(), 43);
        assert!(
            fresh
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        assert_eq!(
            header_json(&json!({"path": "/Çay.epub"})),
            "{\"path\":\"/\\u00c7ay.epub\"}"
        );
        let url = authorize_url(&Hosts::default(), v);
        assert!(
            url.contains("client_id=0gzqlf7pa2k80lv") && url.contains("token_access_type=offline")
        );
    }
}
