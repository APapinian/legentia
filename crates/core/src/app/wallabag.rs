//! Wallabag (sonra okunacak yazılar): okunmamış yazılar Wallabag'in kendi
//! EPUB'ı olarak kütüphanede `Wallabag/` klasörüne iner; "bitti" olarak
//! işaretlenen yazı eşitlemede sunucuda arşivlenir ve cihazdan silinir;
//! sunucuda artık okunmamış olmayan yazı da cihazdan kalkar (yalnızca
//! Legentia'nın indirdikleri). Hesap Araçlar → Hesaplar ve servisler'den
//! girilir; şifre saklanmaz, yalnızca belirteçler (`wallabag.json`, yedeğe girmez).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::form::{Field, Form, FormKind};
use super::jobs::Job;
use super::shelf::ReadStatus;
use super::ui::Rect;
use super::{App, View};
use crate::event::Refresh;
use crate::i18n::Msg;
use crate::net::Net;

pub(super) const WALLABAG_FILE: &str = "wallabag.json";
pub(super) const WALLABAG_DIR: &str = "Wallabag";
/// Bir eşitlemede inecek en çok yazı.
const MAX_ENTRIES: usize = 100;
const PER_PAGE: usize = 50;
const EPUB_LIMIT: u64 = 64 << 20;

/// Kayıtlı hesap ve indirilen yazılar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Wallabag {
    pub server: String,
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    /// Wallabag yazı kimliği → kütüphanedeki dosya adı (`Wallabag/` içinde).
    pub files: BTreeMap<u64, String>,
}

/// Eşitleme sonucu: indirilen, arşivlenen, kaldırılan sayılar ve yeni durum.
pub(super) struct Synced {
    pub state: Wallabag,
    pub added: usize,
    pub archived: usize,
    pub removed: usize,
}

fn form_body(pairs: &[(&str, &str)]) -> Vec<u8> {
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={}", encode(v)))
        .collect::<Vec<_>>()
        .join("&")
        .into_bytes()
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

const FORM: &str = "application/x-www-form-urlencoded";

/// Belirteç ister: şifreyle (ilk giriş) ya da yenileme belirteciyle.
/// (erişim belirteci, yeni yenileme belirteci).
fn token(net: &Net, w: &Wallabag, grant: &[(&str, &str)]) -> Result<(String, String), Msg> {
    let mut pairs = vec![
        ("client_id", w.client_id.as_str()),
        ("client_secret", w.client_secret.as_str()),
    ];
    pairs.extend_from_slice(grant);
    let url = format!("{}/oauth/v2/token", w.server);
    let (status, body) = net
        .send(
            "POST",
            &url,
            &[("Content-Type", FORM.to_string())],
            form_body(&pairs),
            1 << 16,
        )
        .map_err(|e| e.message())?;
    if !(200..300).contains(&status) {
        log::warn!("wallabag token: {status}");
        return Err(Msg::WallabagLogin);
    }
    let v: Value = serde_json::from_slice(&body).map_err(|_| Msg::NetInvalid)?;
    let field = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let access = field("access_token").ok_or(Msg::NetInvalid)?;
    Ok((access, field("refresh_token").unwrap_or_default()))
}

fn api(
    net: &Net,
    w: &Wallabag,
    access: &str,
    method: &str,
    path: &str,
    body: Vec<u8>,
    limit: u64,
) -> Result<Vec<u8>, Msg> {
    let mut headers = vec![("Authorization", format!("Bearer {access}"))];
    if !body.is_empty() {
        headers.push(("Content-Type", FORM.to_string()));
    }
    let (status, out) = net
        .send(
            method,
            &format!("{}{path}", w.server),
            &headers,
            body,
            limit,
        )
        .map_err(|e| e.message())?;
    match status {
        200..=299 => Ok(out),
        401 | 403 => Err(Msg::WallabagLogin),
        s => Err(Msg::NetStatus(s)),
    }
}

/// Okunmamış yazılar: (kimlik, başlık), en yeniden.
fn unread(net: &Net, w: &Wallabag, access: &str) -> Result<Vec<(u64, String)>, Msg> {
    let mut out = Vec::new();
    for page in 1.. {
        let path = format!(
            "/api/entries.json?archive=0&sort=created&order=desc&perPage={PER_PAGE}&page={page}"
        );
        let body = api(net, w, access, "GET", &path, Vec::new(), 8 << 20)?;
        let v: Value = serde_json::from_slice(&body).map_err(|_| Msg::NetInvalid)?;
        let items = v
            .pointer("/_embedded/items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for it in &items {
            if let Some(id) = it.get("id").and_then(Value::as_u64) {
                let title = it.get("title").and_then(Value::as_str).unwrap_or_default();
                out.push((id, title.to_string()));
            }
        }
        let pages = v.get("pages").and_then(Value::as_u64).unwrap_or(1);
        if items.is_empty() || page as u64 >= pages || out.len() >= MAX_ENTRIES {
            break;
        }
    }
    out.truncate(MAX_ENTRIES);
    Ok(out)
}

/// Dosya adı: "Başlık [kimlik].epub" (FAT32'ye uymayan karakterler atılır).
fn file_name(id: u64, title: &str) -> String {
    let clean: String = title
        .chars()
        .map(|c| {
            if c.is_control() || r#"/\:*?"<>|"#.contains(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let clean: String = clean
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(80)
        .collect();
    let clean = clean.trim_start_matches('.').trim();
    format!("{clean} [{id}].epub")
}

/// Ayrı iş parçacığında eşitleme. `finished`: bitti işaretlenmiş yazılar.
fn sync(net: &Net, mut w: Wallabag, dir: &Path, finished: Vec<u64>) -> Result<Synced, Msg> {
    let refresh = w.refresh_token.clone();
    let (access, new_refresh) = token(
        net,
        &w,
        &[("grant_type", "refresh_token"), ("refresh_token", &refresh)],
    )?;
    if !new_refresh.is_empty() {
        w.refresh_token = new_refresh;
    }
    let (mut archived, mut removed, mut added) = (0, 0, 0);
    for id in finished {
        api(
            net,
            &w,
            &access,
            "PATCH",
            &format!("/api/entries/{id}.json"),
            form_body(&[("archive", "1")]),
            1 << 20,
        )?;
        if let Some(name) = w.files.remove(&id) {
            let _ = std::fs::remove_file(dir.join(name));
        }
        archived += 1;
    }
    let list = unread(net, &w, &access)?;
    // Sunucuda artık okunmamış olmayan (başka yerde okunmuş) yazılar kalkar.
    let keep: Vec<u64> = list.iter().map(|(id, _)| *id).collect();
    let gone: Vec<u64> = w
        .files
        .keys()
        .copied()
        .filter(|id| !keep.contains(id))
        .collect();
    for id in gone {
        if let Some(name) = w.files.remove(&id) {
            let _ = std::fs::remove_file(dir.join(name));
            removed += 1;
        }
    }
    std::fs::create_dir_all(dir).map_err(|_| Msg::NetDisk)?;
    for (id, title) in list {
        if w.files.contains_key(&id) {
            continue;
        }
        let epub = api(
            net,
            &w,
            &access,
            "GET",
            &format!("/api/entries/{id}/export.epub"),
            Vec::new(),
            EPUB_LIMIT,
        )?;
        let name = file_name(id, &title);
        std::fs::write(dir.join(&name), epub).map_err(|_| Msg::NetDisk)?;
        w.files.insert(id, name);
        added += 1;
    }
    Ok(Synced {
        state: w,
        added,
        archived,
        removed,
    })
}

/// İlk giriş: şifreyle belirteç alınır, şifre saklanmaz.
fn login(net: &Net, mut w: Wallabag, user: &str, password: &str) -> Result<Wallabag, Msg> {
    let (_, refresh) = token(
        net,
        &w,
        &[
            ("grant_type", "password"),
            ("username", user),
            ("password", password),
        ],
    )?;
    if refresh.is_empty() {
        return Err(Msg::WallabagLogin);
    }
    w.refresh_token = refresh;
    Ok(w)
}

/// Arka plan işleri.
pub(super) enum WallabagJob {
    Login(Job<Result<Wallabag, Msg>>),
    Sync(Job<Result<Synced, Msg>>),
}

impl App {
    pub(super) fn wallabag_button(&self) -> Rect {
        self.service_row(2)
    }

    pub(super) fn wallabag_account_button(&self) -> Rect {
        self.service_row(3)
    }

    fn wallabag_dir(&self) -> PathBuf {
        self.root.join(WALLABAG_DIR)
    }

    pub(super) fn open_wallabag(&mut self) -> Option<Refresh> {
        if !self.settings.network {
            self.show_message(Msg::NetworkOff);
            return Some(self.refresh(true));
        }
        match &self.wallabag {
            Some(w) if !w.refresh_token.is_empty() => self.sync_wallabag(),
            _ => self.open_wallabag_form(),
        }
    }

    pub(super) fn open_wallabag_form(&mut self) -> Option<Refresh> {
        let w = self.wallabag.clone().unwrap_or_default();
        let mut secret = Field::new(Msg::FieldClientSecret, &w.client_secret);
        secret.secret = true;
        let mut password = Field::new(Msg::FieldPassword, "");
        password.secret = true;
        let fields = vec![
            Field::new(Msg::FieldServer, &w.server),
            Field::new(Msg::FieldClientId, &w.client_id),
            secret,
            Field::new(Msg::FieldUser, ""),
            password,
        ];
        self.open_form(Form::new(FormKind::Wallabag, fields))
    }

    /// Hesap formu: girişi arka planda dener.
    pub(super) fn submit_wallabag(&mut self, form: &Form) -> Result<(), Msg> {
        let value = |i: usize| form.fields[i].value.trim().to_string();
        let server = value(0).trim_end_matches('/').to_string();
        if !(server.starts_with("https://") || server.starts_with("http://"))
            || server.contains(' ')
        {
            return Err(Msg::InvalidUrl);
        }
        let w = Wallabag {
            server,
            client_id: value(1),
            client_secret: value(2),
            files: self
                .wallabag
                .as_ref()
                .map(|w| w.files.clone())
                .unwrap_or_default(),
            ..Default::default()
        };
        let (user, password) = (value(3), form.fields[4].value.clone());
        let net = self.browser.net();
        self.wallabag_job = Some(WallabagJob::Login(Job::spawn(move |_| {
            login(&net, w, &user, &password)
        })));
        self.view = View::Working;
        Ok(())
    }

    fn sync_wallabag(&mut self) -> Option<Refresh> {
        let w = self.wallabag.clone()?;
        // "Bitti" işaretlenen ve Legentia'nın indirdiği yazılar arşivlenecek.
        let dir = self.wallabag_dir();
        let finished: Vec<u64> = w
            .files
            .iter()
            .filter(|(_, name)| {
                let path = dir.join(name);
                self.library
                    .iter()
                    .find(|e| e.path == path)
                    .is_some_and(|e| self.status_of(&e.id) == ReadStatus::Finished)
            })
            .map(|(id, _)| *id)
            .collect();
        let net = self.browser.net();
        self.wallabag_job = Some(WallabagJob::Sync(Job::spawn(move |_| {
            sync(&net, w, &dir, finished)
        })));
        self.view = View::Working;
        Some(self.refresh(true))
    }

    fn save_wallabag(&self) {
        if let Some(w) = &self.wallabag {
            self.store.save(WALLABAG_FILE, w);
        }
    }

    /// `Event::Tick`: giriş ya da eşitleme bittiyse sonucu.
    pub(super) fn poll_wallabag(&mut self) -> Option<Refresh> {
        let back = super::Back::Library;
        let msg = match self.wallabag_job.as_ref()? {
            WallabagJob::Login(job) => match job.poll()?.unwrap_or(Err(Msg::NetInvalid)) {
                Ok(w) => {
                    self.wallabag = Some(w);
                    self.save_wallabag();
                    self.wallabag_job = None;
                    return self.sync_wallabag();
                }
                Err(m) => m,
            },
            WallabagJob::Sync(job) => match job.poll()?.unwrap_or(Err(Msg::NetInvalid)) {
                Ok(s) => {
                    self.wallabag = Some(s.state);
                    self.save_wallabag();
                    self.rescan();
                    Msg::WallabagSynced {
                        added: s.added,
                        archived: s.archived,
                        removed: s.removed,
                    }
                }
                Err(m) => m,
            },
        };
        self.wallabag_job = None;
        self.view = View::Message { msg, back };
        Some(self.refresh(true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_carry_the_entry_id_and_are_fat_safe() {
        assert_eq!(file_name(7, "Neden: \"Okumak\"?"), "Neden Okumak [7].epub");
        assert_eq!(file_name(8, ".gizli"), "gizli [8].epub");
        assert_eq!(
            String::from_utf8(form_body(&[("a", "b c&ç")])).unwrap(),
            "a=b%20c%26%C3%A7"
        );
    }
}
