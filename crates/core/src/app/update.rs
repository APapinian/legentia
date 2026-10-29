//! Cihazda güncelleme (Araçlar → Güncellemeleri denetle): GitHub'daki son
//! sürüm sorulur, kullanıcı onaylarsa programın yeni hâli indirilir, SHA-256
//! özeti doğrulanır ve programın yanına `legentia-kobo.yeni` olarak konur.
//! Kurulum bir sonraki açılışta `legentia.sh`'dedir: eski program
//! `legentia-kobo.eski` olarak saklanır, yeni sürüm hemen hatayla kapanırsa
//! eskisine dönülür. Yalnızca cihaz katmanı güncelleme klasörünü verdiyse
//! çalışır (simülatörde değiştirilecek program yoktur).
//!
//! Sürümde beklenen dosyalar: `legentia-kobo` (statik ARMv7 program) ve
//! `legentia-kobo.sha256` (özet; `sha256sum` çıktısı biçiminde).

use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::jobs::Job;
use super::ui::{
    HEADER_BOTTOM, MARGIN, PLAIN, ROW_PX, Rect, back_button, draw_button, draw_header,
};
use super::{App, View};
use crate::canvas::Canvas;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};
use crate::net::Net;
use crate::text::Family;

pub(super) const LATEST: &str = "https://api.github.com/repos/APapinian/legentia/releases/latest";
const BINARY: &str = "legentia-kobo";
const CHECKSUM: &str = "legentia-kobo.sha256";
const NEW_FILE: &str = "legentia-kobo.yeni";
const MAX_BINARY: u64 = 64 << 20;

/// Bulunan sürüm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Release {
    pub tag: String,
    pub notes: String,
    pub binary: String,
    pub checksum: String,
}

pub(super) enum Step {
    /// GitHub'a soruluyor.
    Checking(Job<Result<Option<Release>, Msg>>),
    /// Yeni sürüm var; onay bekleniyor.
    Found(Release),
    /// İndirilip doğrulanıyor.
    Downloading(Job<Result<(), Msg>>),
}

/// "v1.2.3" → (1, 2, 3); sayı olmayan parça 0.
fn version(tag: &str) -> (u64, u64, u64) {
    let mut parts = tag
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(['.', '-', '+'])
        .map(|p| p.parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// Son sürüm yanıtından, bundan yeniyse ve gerekli dosyaları taşıyorsa sürüm.
fn parse_release(json: &[u8], current: &str) -> Result<Option<Release>, Msg> {
    let v: Value = serde_json::from_slice(json).map_err(|_| Msg::NetInvalid)?;
    let tag = v
        .get("tag_name")
        .and_then(Value::as_str)
        .ok_or(Msg::NetInvalid)?;
    if version(tag) <= version(current) {
        return Ok(None);
    }
    let asset = |name: &str| {
        v.get("assets")?
            .as_array()?
            .iter()
            .find(|a| a.get("name").and_then(Value::as_str) == Some(name))?
            .get("browser_download_url")?
            .as_str()
            .map(str::to_string)
    };
    let (Some(binary), Some(checksum)) = (asset(BINARY), asset(CHECKSUM)) else {
        return Err(Msg::UpdateIncomplete);
    };
    Ok(Some(Release {
        tag: tag.to_string(),
        notes: v
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        binary,
        checksum,
    }))
}

fn check(net: &Net, url: &str) -> Result<Option<Release>, Msg> {
    let body = net.get(url, None, 1 << 20).map_err(|e| {
        log::warn!("update check: {e:?}");
        match e {
            crate::net::NetError::Status(404) => Msg::NoReleases,
            e => e.message(),
        }
    })?;
    parse_release(&body, env!("CARGO_PKG_VERSION"))
}

fn sha256_hex(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Programı indirir, özeti doğrular; uymazsa dosyayı siler.
fn fetch(net: &Net, release: &Release, dir: &Path) -> Result<(), Msg> {
    let sum = net
        .get(&release.checksum, None, 4096)
        .map_err(|e| e.message())?;
    let expected = String::from_utf8_lossy(&sum)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if expected.len() != 64 {
        return Err(Msg::UpdateIncomplete);
    }
    let dest = dir.join(NEW_FILE);
    net.download(&release.binary, None, &dest, MAX_BINARY, |_, _| true)
        .map_err(|e| e.message())?;
    let actual = sha256_hex(&dest).map_err(|_| Msg::NetDisk)?;
    if actual != expected {
        log::error!("update checksum mismatch: {actual} != {expected}");
        let _ = std::fs::remove_file(&dest);
        return Err(Msg::UpdateBadChecksum);
    }
    log::info!("update {} downloaded to {}", release.tag, dest.display());
    Ok(())
}

impl App {
    /// Cihaz katmanı: güncellemenin konacağı (programın bulunduğu) klasör.
    pub fn set_update_dir(&mut self, dir: PathBuf) {
        self.update_dir = Some(dir);
    }

    pub(super) fn update_button(&self) -> Rect {
        self.tool_row(7)
    }

    pub(super) fn check_update(&mut self) -> Option<Refresh> {
        if self.update_dir.is_none() {
            self.show_message(Msg::UpdateDeviceOnly);
            return Some(self.refresh(true));
        }
        if !self.settings.network {
            self.show_message(Msg::NetworkOff);
            return Some(self.refresh(true));
        }
        let net = self.browser.net();
        let url = self.endpoints.releases.clone();
        self.update = Some(Step::Checking(Job::spawn(move |_| check(&net, &url))));
        self.view = View::Update;
        Some(self.refresh(true))
    }

    /// `Event::Tick`: denetleme ya da indirme bittiyse sonucu.
    pub(super) fn poll_update(&mut self) -> Option<Refresh> {
        let msg = match self.update.as_ref()? {
            Step::Checking(job) => match job.poll()?.unwrap_or(Err(Msg::NetInvalid)) {
                Ok(Some(release)) => {
                    self.update = Some(Step::Found(release));
                    return Some(self.refresh(true));
                }
                Ok(None) => Msg::UpToDate(env!("CARGO_PKG_VERSION").to_string()),
                Err(m) => m,
            },
            Step::Downloading(job) => match job.poll()?.unwrap_or(Err(Msg::NetInvalid)) {
                Ok(()) => Msg::UpdateReady,
                Err(m) => m,
            },
            Step::Found(_) => return None,
        };
        self.update = None;
        self.view = View::Message {
            msg,
            back: super::Back::Library,
        };
        Some(self.refresh(true))
    }

    pub(super) fn install_button(&self) -> Rect {
        Rect {
            x: MARGIN,
            y: crate::LIBRA2_HEIGHT as i32 - 260,
            w: self.width as i32 - 2 * MARGIN,
            h: 110,
        }
    }

    pub(super) fn handle_update(&mut self, event: Event) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        let back = back_button(self.width).contains(x, y);
        match self.update.as_ref() {
            Some(Step::Found(release)) if self.install_button().contains(x, y) => {
                let (release, dir) = (release.clone(), self.update_dir.clone()?);
                let net = self.browser.net();
                self.update = Some(Step::Downloading(Job::spawn(move |_| {
                    fetch(&net, &release, &dir)
                })));
                Some(self.refresh(false))
            }
            Some(Step::Found(_)) if back => {
                self.update = None;
                self.view = View::Backup;
                Some(self.refresh(true))
            }
            _ => None,
        }
    }

    pub(super) fn draw_update(&self, canvas: &mut Canvas) {
        let t = &self.text;
        let current = env!("CARGO_PKG_VERSION").to_string();
        draw_header(
            t,
            canvas,
            &tr(&Msg::Update),
            &tr(&Msg::CurrentVersion(current)),
            true,
        );
        let y = HEADER_BOTTOM + 80;
        match &self.update {
            Some(Step::Found(r)) => {
                draw_button(
                    t,
                    canvas,
                    back_button(canvas.width()),
                    &tr(&Msg::Cancel),
                    ROW_PX,
                );
                t.draw_msg(canvas, &Msg::NewVersion(r.tag.clone()), MARGIN, y, ROW_PX);
                let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
                let bottom = self.install_button().y - 40;
                let mut ly = y + 70;
                for line in r
                    .notes
                    .lines()
                    .flat_map(|l| t.wrap_plain(l, Family::Ui, PLAIN, 30.0, max_w))
                {
                    if ly > bottom {
                        break;
                    }
                    t.draw_plain(canvas, &line, Family::Ui, PLAIN, MARGIN, ly, 30.0);
                    ly += 44;
                }
                draw_button(
                    t,
                    canvas,
                    self.install_button(),
                    &tr(&Msg::InstallUpdate),
                    ROW_PX,
                );
            }
            Some(Step::Downloading(_)) => t.draw_msg(canvas, &Msg::Downloading, MARGIN, y, ROW_PX),
            _ => t.draw_msg(canvas, &Msg::Loading, MARGIN, y, ROW_PX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_releases_with_both_files_are_offered() {
        assert!(version("v0.2.0") > version("0.1.9"));
        assert!(version("v1.0") > version("0.9.9"));
        assert_eq!(version("v0.1.0-beta"), (0, 1, 0));
        let json = |tag: &str, assets: &str| {
            format!(r#"{{"tag_name":"{tag}","body":"Yenilikler","assets":[{assets}]}}"#)
        };
        let both = r#"{"name":"legentia-kobo","browser_download_url":"https://x/b"},{"name":"legentia-kobo.sha256","browser_download_url":"https://x/s"}"#;
        let r = parse_release(json("v9.0.0", both).as_bytes(), "0.1.0")
            .unwrap()
            .unwrap();
        assert_eq!(
            (r.binary.as_str(), r.checksum.as_str()),
            ("https://x/b", "https://x/s")
        );
        assert_eq!(
            parse_release(json("v0.1.0", both).as_bytes(), "0.1.0"),
            Ok(None)
        );
        let only = r#"{"name":"legentia-kobo","browser_download_url":"https://x/b"}"#;
        assert_eq!(
            parse_release(json("v9.0.0", only).as_bytes(), "0.1.0"),
            Err(Msg::UpdateIncomplete)
        );
    }
}
