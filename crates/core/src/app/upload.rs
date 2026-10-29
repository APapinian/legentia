//! Kablosuz yükleme (Araçlar → Kablosuz yükleme): ekran açıkken aynı ağdaki
//! bilgisayarın tarayıcısından kitap gönderilir, `<kütüphane>/İndirilenler`'e
//! kaydedilir. Bağımlılıksız küçük bir HTTP sunucusudur:
//!
//! - `GET /` yükleme sayfası; sayfa dosyayı `PUT /yukle?ad=…&kod=…` ile tek
//!   istekte gönderir (çok parçalı form ayrıştırılmaz).
//! - Her açılışta yeni, 6 haneli kod; kodsuz ya da yanlış kodlu yükleme
//!   reddedilir, 10 yanlış denemede sunucu kapanır.
//! - Yalnızca kitap uzantıları; ad temizlenir (klasör ayırıcı, baştaki nokta
//!   atılır), var olan dosyanın üzerine yazılmaz; boyut sınırı ve boş alan
//!   denetlenir; yarım kalan yükleme iz bırakmaz.
//! - Ekrandan çıkınca sunucu kapanır.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use super::ui::{BOLD, HEADER_BOTTOM, MARGIN, ROW_PX, back_button, draw_button, draw_header};
use super::{App, View};
use crate::canvas::Canvas;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};
use crate::text::Family;

const FIRST_PORT: u16 = 8080;
const MAX_UPLOAD: u64 = 1024 * 1024 * 1024;
const MAX_HEADER: usize = 16 * 1024;
const MAX_WRONG_CODES: u32 = 10;
/// Kabul edilen uzantılar (küçük harf; `.fb2.zip` adın sonuna bakılarak).
const EXTENSIONS: [&str; 9] = [
    "epub", "pdf", "fb2", "txt", "html", "htm", "cbz", "cbr", "zip",
];

/// Sunucudan uygulamaya gelen haber.
enum Report {
    Saved(String),
    Rejected(String),
    /// Çok yanlış kod denendi; sunucu kapandı.
    Locked,
}

pub(super) struct Server {
    pub address: String,
    pub code: String,
    /// Kaydedilen dosyalar (ad).
    pub saved: Vec<String>,
    /// Son reddedilen istek (neden).
    pub rejected: Option<String>,
    pub locked: bool,
    stop: Arc<AtomicBool>,
    rx: Receiver<Report>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Bu cihazın yerel ağ adresi: dışarıya bağlanır gibi yapan UDP soketinin
/// kaynak adresi (paket gönderilmez).
pub(super) fn local_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then_some(ip)
}

/// Saatten ve süreç kimliğinden üretilen 6 haneli kod (gizlilik için değil,
/// aynı ağdaki başkası yanlışlıkla ya da tahminle yükleyemesin diye).
pub(super) fn new_code() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let mut x = nanos ^ (std::process::id() as u64).rotate_left(32) ^ 0x9E37_79B9_7F4A_7C15;
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    x ^= x >> 33;
    format!("{:06}", x % 1_000_000)
}

/// Gelen dosya adını güvenli bir ada çevirir; kabul edilmeyen uzantıda `None`.
fn clean_name(name: &str) -> Option<String> {
    let base = name.rsplit(['/', '\\']).next()?.trim();
    let base: String = base
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect();
    let base = base.trim_start_matches('.').trim().to_string();
    let lower = base.to_lowercase();
    let ext = Path::new(&lower).extension()?.to_str()?.to_string();
    let ok = EXTENSIONS.contains(&ext.as_str()) && (ext != "zip" || lower.ends_with(".fb2.zip"));
    (ok && base.len() > ext.len() + 1).then_some(base)
}

/// `dir` içinde `name` boşsa o, değilse "ad (2).uzantı" gibi boş bir yol.
fn free_path(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    if !path.exists() {
        return path;
    }
    let (stem, ext) = match name.find('.') {
        Some(i) => (&name[..i], &name[i..]),
        None => (name, ""),
    };
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or(path)
}

/// `%XX` ve `+` çözülmüş sorgu değeri.
pub(super) fn query_value(query: &str, key: &str) -> Option<String> {
    let raw = query
        .split('&')
        .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))?;
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

pub(super) fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &str) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}

/// Yükleme sayfası; metinler arayüz dilinden.
fn page() -> String {
    let t = |m: Msg| tr(&m).replace('<', "&lt;");
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Legentia</title>
<style>body{{font-family:sans-serif;max-width:40em;margin:2em auto;padding:0 1em}}#alan{{border:3px dashed #888;padding:3em;text-align:center}}input{{font-size:1.2em}}</style></head>
<body><h1>Legentia</h1><p>{hint}</p>
<p><label>{code}: <input id="kod" inputmode="numeric" maxlength="6" size="6"></label></p>
<div id="alan"><p>{drop}</p><input type="file" id="dosya" multiple></div><ul id="liste"></ul>
<script>
const liste=document.getElementById('liste');
async function gonder(f){{const li=document.createElement('li');li.textContent=f.name+' …';liste.appendChild(li);
const kod=document.getElementById('kod').value.trim();
try{{const r=await fetch('/yukle?ad='+encodeURIComponent(f.name)+'&kod='+encodeURIComponent(kod),{{method:'PUT',body:f}});
li.textContent=f.name+' — '+(await r.text());}}catch(e){{li.textContent=f.name+' — '+e;}}}}
async function hepsi(fs){{for(const f of fs){{await gonder(f);}}}}
document.getElementById('dosya').onchange=e=>hepsi(e.target.files);
const a=document.getElementById('alan');a.ondragover=e=>e.preventDefault();
a.ondrop=e=>{{e.preventDefault();hepsi(e.dataTransfer.files);}};
</script></body></html>"#,
        hint = t(Msg::UploadPageHint),
        code = t(Msg::UploadCode),
        drop = t(Msg::UploadDrop),
    )
}

/// Bir bağlantıyı işler; yanlış kodda `true`.
fn serve(mut stream: TcpStream, code: &str, dir: &Path, tx: &Sender<Report>, page: &str) -> bool {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return false,
    });
    let mut head = Vec::new();
    // Başlıklar boş satıra kadar (sınırlı).
    loop {
        let mut line = Vec::new();
        match reader
            .by_ref()
            .take(MAX_HEADER as u64)
            .read_until(b'\n', &mut line)
        {
            Ok(0) | Err(_) => return false,
            Ok(_) => {}
        }
        if line == b"\r\n" || line == b"\n" {
            break;
        }
        head.extend_from_slice(&line);
        if head.len() > MAX_HEADER {
            respond(&mut stream, "431 Too Large", "text/plain", "");
            return false;
        }
    }
    let head = String::from_utf8_lossy(&head);
    let mut lines = head.lines();
    let mut first = lines.next().unwrap_or_default().split_whitespace();
    let (method, target) = (
        first.next().unwrap_or_default(),
        first.next().unwrap_or("/"),
    );
    let length = lines
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse::<u64>().ok());
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    match (method, path) {
        ("GET", "/") => {
            respond(&mut stream, "200 OK", "text/html", page);
            false
        }
        ("PUT", "/yukle") => {
            if query_value(query, "kod").as_deref() != Some(code) {
                respond(
                    &mut stream,
                    "403 Forbidden",
                    "text/plain",
                    &tr(&Msg::UploadWrongCode),
                );
                return true;
            }
            let reply = receive(&mut reader, query, length, dir);
            match reply {
                Ok(name) => {
                    respond(&mut stream, "200 OK", "text/plain", &tr(&Msg::UploadSaved));
                    let _ = tx.send(Report::Saved(name));
                }
                Err(msg) => {
                    let text = tr(&msg);
                    respond(&mut stream, "400 Bad Request", "text/plain", &text);
                    let _ = tx.send(Report::Rejected(text));
                }
            }
            false
        }
        _ => {
            respond(&mut stream, "404 Not Found", "text/plain", "");
            false
        }
    }
}

/// İsteğin gövdesini `dir`'e kaydeder: önce geçici dosyaya, bitince adına.
fn receive(
    body: &mut impl Read,
    query: &str,
    length: Option<u64>,
    dir: &Path,
) -> Result<String, Msg> {
    let name = query_value(query, "ad")
        .and_then(|n| clean_name(&n))
        .ok_or(Msg::UploadBadFile)?;
    let length = length.ok_or(Msg::UploadBadFile)?;
    if length > MAX_UPLOAD {
        return Err(Msg::UploadTooLarge);
    }
    std::fs::create_dir_all(dir).map_err(|_| Msg::UploadFailed)?;
    if crate::storage::free_space(dir).is_some_and(|free| free < length + 16 * 1024 * 1024) {
        return Err(Msg::UploadNoSpace);
    }
    let path = free_path(dir, &name);
    let temp = path.with_extension("yukleniyor");
    let result = (|| {
        let mut file = std::fs::File::create(&temp)?;
        let copied = std::io::copy(&mut body.take(length), &mut file)?;
        if copied != length {
            return Err(std::io::Error::other("short body"));
        }
        file.sync_all()?;
        std::fs::rename(&temp, &path)
    })();
    match result {
        Ok(()) => {
            log::info!("uploaded {}", path.display());
            Ok(path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or(name))
        }
        Err(e) => {
            log::warn!("upload failed: {e}");
            let _ = std::fs::remove_file(&temp);
            Err(Msg::UploadFailed)
        }
    }
}

impl Server {
    /// Sunucuyu `dir`'e kaydetmek üzere başlatır; `ip` ekranda gösterilecek
    /// adres. Ağ yoksa ya da port açılamazsa `None`.
    pub(super) fn start(dir: PathBuf, ip: Option<std::net::IpAddr>) -> Option<Server> {
        let ip = ip?;
        let listener =
            (FIRST_PORT..FIRST_PORT + 20).find_map(|p| TcpListener::bind(("0.0.0.0", p)).ok())?;
        let port = listener.local_addr().ok()?.port();
        listener.set_nonblocking(true).ok()?;
        let code = new_code();
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let (flag, secret) = (stop.clone(), code.clone());
        let page = page();
        std::thread::spawn(move || {
            let mut wrong = 0;
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        if serve(stream, &secret, &dir, &tx, &page) {
                            wrong += 1;
                            if wrong >= MAX_WRONG_CODES {
                                let _ = tx.send(Report::Locked);
                                break;
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    Err(_) => break,
                }
            }
        });
        Some(Server {
            address: format!("http://{ip}:{port}"),
            code,
            saved: Vec::new(),
            rejected: None,
            locked: false,
            stop,
            rx,
        })
    }
}

impl App {
    pub(super) fn upload_button(&self) -> super::ui::Rect {
        self.tool_row(3)
    }

    pub(super) fn open_upload(&mut self) -> Option<Refresh> {
        if !self.settings.network {
            self.view = View::Message {
                msg: Msg::NetworkOff,
                back: super::Back::Library,
            };
            return Some(self.refresh(true));
        }
        self.upload = Server::start(self.root.join(super::downloads::DOWNLOAD_DIR), local_ip());
        if self.upload.is_none() {
            self.view = View::Message {
                msg: Msg::UploadNoNetwork,
                back: super::Back::Library,
            };
            return Some(self.refresh(true));
        }
        self.view = View::Upload;
        Some(self.refresh(true))
    }

    /// `Event::Tick`: sunucudan gelen haberler; kitap geldiyse kütüphane yenilenir.
    pub(super) fn poll_upload(&mut self) -> Option<Refresh> {
        let server = self.upload.as_mut()?;
        let mut changed = false;
        let mut saved = false;
        while let Ok(report) = server.rx.try_recv() {
            changed = true;
            match report {
                Report::Saved(name) => {
                    server.saved.push(name);
                    saved = true;
                }
                Report::Rejected(why) => server.rejected = Some(why),
                Report::Locked => server.locked = true,
            }
        }
        if saved {
            self.rescan();
        }
        (changed && matches!(self.view, View::Upload)).then(|| self.refresh(false))
    }

    pub(super) fn handle_upload(&mut self, event: Event) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        if !back_button(self.width).contains(x, y) {
            return None;
        }
        self.upload = None;
        self.view = View::Library { page: 0 };
        Some(self.refresh(true))
    }

    pub(super) fn draw_upload(&self, canvas: &mut Canvas) {
        let t = &self.text;
        draw_header(t, canvas, &tr(&Msg::Upload), "", true);
        draw_button(
            t,
            canvas,
            back_button(canvas.width()),
            &tr(&Msg::Back),
            ROW_PX,
        );
        let Some(s) = &self.upload else {
            return;
        };
        let mut y = HEADER_BOTTOM + 60;
        self.draw_message_lines(canvas, &Msg::UploadHint, y);
        y += 240;
        t.draw_plain(canvas, &s.address, Family::Ui, BOLD, MARGIN, y, 52.0);
        y += 110;
        t.draw_msg(
            canvas,
            &Msg::UploadCodeLine(s.code.clone()),
            MARGIN,
            y,
            52.0,
        );
        y += 110;
        if s.locked {
            self.draw_message_lines(canvas, &Msg::UploadLocked, y);
            return;
        }
        if let Some(why) = &s.rejected {
            t.draw_plain(canvas, why, Family::Ui, BOLD, MARGIN, y, 30.0);
            y += 60;
        }
        t.draw_msg(canvas, &Msg::UploadCount(s.saved.len()), MARGIN, y, ROW_PX);
        y += 60;
        for name in s.saved.iter().rev().take(8) {
            let line = super::ui::one_line(
                t,
                name,
                super::ui::PLAIN,
                30.0,
                (canvas.width() as i32 - 2 * MARGIN) as f32,
            );
            t.draw_plain(canvas, &line, Family::Ui, super::ui::PLAIN, MARGIN, y, 30.0);
            y += 48;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_cleaned_and_limited_to_books() {
        assert_eq!(
            clean_name("Çalıkuşu.epub").as_deref(),
            Some("Çalıkuşu.epub")
        );
        assert_eq!(clean_name("../../etc/x.pdf").as_deref(), Some("x.pdf"));
        assert_eq!(clean_name("C:\\a\\b.cbz").as_deref(), Some("b.cbz"));
        assert_eq!(clean_name(".gizli.epub").as_deref(), Some("gizli.epub"));
        assert_eq!(
            clean_name("kitap.fb2.zip").as_deref(),
            Some("kitap.fb2.zip")
        );
        assert_eq!(clean_name("arsiv.zip"), None);
        assert_eq!(clean_name("betik.sh"), None);
        assert_eq!(clean_name(".epub"), None);
        assert_eq!(
            query_value("ad=G%C3%BCzel+Kitap.epub&kod=123456", "ad").as_deref(),
            Some("Güzel Kitap.epub")
        );
        assert_eq!(query_value("kod=12", "ad"), None);
        assert_eq!(new_code().len(), 6);
    }
}
