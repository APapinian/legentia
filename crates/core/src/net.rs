//! İnternet: HTTP(S) istemcisi. TLS rustls + ring; kök sertifikalar uygulamaya
//! gömülüdür (webpki-roots, Mozilla listesi), cihazın eski sertifika paketine
//! bağlı kalınmaz. Sertifika doğrulaması hiçbir durumda kapatılmaz. Yerel ağdaki
//! Calibre sunucusu için düz HTTP'ye izin verilir.
//!
//! Hatalar kullanıcıya Türkçe anlatılır ([`NetError::message`]); bağlantı
//! kesilse de yerel kitaplar ve notlar etkilenmez: indirme önce geçici dosyaya
//! yazılır, bitince yerine taşınır.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::i18n::Msg;

/// Sunucuya bağlanma ve yanıtın başlaması için süre sınırları.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const USER_AGENT: &str = concat!("Legentia/", env!("CARGO_PKG_VERSION"));
/// İndirilen dosya, bitene kadar bu uzantıyla yazılır.
const PARTIAL: &str = "indiriliyor";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Auth {
    pub user: String,
    pub password: String,
}

#[derive(Debug)]
pub enum NetError {
    /// Ad çözülemedi ya da bağlantı kurulamadı (Wi-Fi yok, sunucu kapalı).
    Offline(String),
    Timeout,
    /// TLS el sıkışması ya da sertifika doğrulaması başarısız.
    Tls(String),
    Unauthorized,
    Status(u16),
    TooLarge,
    Invalid(String),
    Io(std::io::Error),
    Cancelled,
}

impl NetError {
    pub fn message(&self) -> Msg {
        match self {
            NetError::Offline(_) => Msg::NetOffline,
            NetError::Timeout => Msg::NetTimeout,
            NetError::Tls(_) => Msg::NetTls,
            NetError::Unauthorized => Msg::NetUnauthorized,
            NetError::Status(code) => Msg::NetStatus(*code),
            NetError::TooLarge => Msg::NetTooLarge,
            NetError::Invalid(_) => Msg::NetInvalid,
            NetError::Io(_) => Msg::NetDisk,
            NetError::Cancelled => Msg::NetCancelled,
        }
    }
}

impl From<ureq::Error> for NetError {
    fn from(e: ureq::Error) -> Self {
        match e {
            ureq::Error::Timeout(_) => NetError::Timeout,
            ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => {
                NetError::Offline(e.to_string())
            }
            ureq::Error::Rustls(_) | ureq::Error::Tls(_) => NetError::Tls(e.to_string()),
            ureq::Error::BodyExceedsLimit(_) => NetError::TooLarge,
            ureq::Error::StatusCode(401) => NetError::Unauthorized,
            ureq::Error::StatusCode(c) => NetError::Status(c),
            ureq::Error::Io(io) => match io.kind() {
                std::io::ErrorKind::TimedOut => NetError::Timeout,
                std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::NotConnected => NetError::Offline(io.to_string()),
                // rustls hataları ureq'te çoğu zaman G/Ç hatası olarak gelir.
                _ if io.to_string().contains("certificate") => NetError::Tls(io.to_string()),
                _ => NetError::Offline(io.to_string()),
            },
            other => NetError::Invalid(other.to_string()),
        }
    }
}

#[derive(Clone)]
pub struct Net {
    agent: ureq::Agent,
}

impl Default for Net {
    fn default() -> Self {
        Self::with_timeouts(CONNECT_TIMEOUT, RESPONSE_TIMEOUT)
    }
}

impl Net {
    pub fn with_timeouts(connect: Duration, response: Duration) -> Net {
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(connect))
            .timeout_recv_response(Some(response))
            .http_status_as_error(false)
            // WebDAV'ın PROPFIND, MKCOL gibi yöntemleri için.
            .allow_non_standard_methods(true)
            .user_agent(USER_AGENT)
            .build();
        Net {
            agent: ureq::Agent::new_with_config(config),
        }
    }

    /// GET isteği. Yetki bilgisi hemen gönderilmez: sunucu 401 ile yöntemini
    /// bildirince (Digest ya da Basic) onunla yeniden denenir. Böylece Digest
    /// isteyen sunucuya (ör. düz HTTP'deki Calibre) şifre açık gitmez.
    fn request(
        &self,
        url: &str,
        auth: Option<&Auth>,
    ) -> Result<ureq::http::Response<ureq::Body>, NetError> {
        let mut resp = self.agent.get(url).call()?;
        if let (401, Some(a)) = (resp.status().as_u16(), auth) {
            let challenges: Vec<String> = resp
                .headers()
                .get_all("www-authenticate")
                .iter()
                .filter_map(|v| v.to_str().ok().map(str::to_string))
                .collect();
            let header = authorization("GET", &challenges, a, url)?;
            resp = self.agent.get(url).header("Authorization", header).call()?;
        }
        match resp.status().as_u16() {
            200..=299 => Ok(resp),
            401 | 403 => Err(NetError::Unauthorized),
            code => Err(NetError::Status(code)),
        }
    }

    /// Herhangi bir yöntemle, başlıklar ve gövdeyle istek; (durum kodu,
    /// en fazla `limit` baytlık gövde). Durum kodunu çağıran yorumlar
    /// (çeviri ve WebDAV kendi hata kodlarına bakar).
    pub fn send(
        &self,
        method: &str,
        url: &str,
        headers: &[(&str, String)],
        body: Vec<u8>,
        limit: u64,
    ) -> Result<(u16, Vec<u8>), NetError> {
        let mut b = ureq::http::Request::builder().method(method).uri(url);
        for (k, v) in headers {
            b = b.header(*k, v);
        }
        let req = b.body(body).map_err(|e| NetError::Invalid(e.to_string()))?;
        let mut resp = self.agent.run(req)?;
        let status = resp.status().as_u16();
        Self::read_limited(&mut resp, limit).map(|out| (status, out))
    }

    /// `send`, ama yetki bilgisi `request` gibi yalnızca sunucu 401 ile
    /// isteyince (Basic ya da yönteme göre Digest) gönderilir.
    pub fn send_auth(
        &self,
        method: &str,
        url: &str,
        headers: &[(&str, String)],
        body: Vec<u8>,
        limit: u64,
        auth: Option<&Auth>,
    ) -> Result<(u16, Vec<u8>), NetError> {
        let build = |extra: Option<String>, body: Vec<u8>| {
            let mut b = ureq::http::Request::builder().method(method).uri(url);
            for (k, v) in headers {
                b = b.header(*k, v);
            }
            if let Some(a) = extra {
                b = b.header("Authorization", a);
            }
            b.body(body).map_err(|e| NetError::Invalid(e.to_string()))
        };
        let mut resp = self.agent.run(build(None, body.clone())?)?;
        if let (401, Some(a)) = (resp.status().as_u16(), auth) {
            let challenges: Vec<String> = resp
                .headers()
                .get_all("www-authenticate")
                .iter()
                .filter_map(|v| v.to_str().ok().map(str::to_string))
                .collect();
            let header = authorization(method, &challenges, a, url)?;
            resp = self.agent.run(build(Some(header), body)?)?;
        }
        let status = resp.status().as_u16();
        Self::read_limited(&mut resp, limit).map(|out| (status, out))
    }

    fn read_limited(
        resp: &mut ureq::http::Response<ureq::Body>,
        limit: u64,
    ) -> Result<Vec<u8>, NetError> {
        let mut out = Vec::new();
        resp.body_mut()
            .as_reader()
            .take(limit + 1)
            .read_to_end(&mut out)
            .map_err(|e| NetError::from(ureq::Error::Io(e)))?;
        if out.len() as u64 > limit {
            return Err(NetError::TooLarge);
        }
        Ok(out)
    }

    /// Yanıtın gövdesi (en fazla `limit` bayt).
    pub fn get(&self, url: &str, auth: Option<&Auth>, limit: u64) -> Result<Vec<u8>, NetError> {
        let mut resp = self.request(url, auth)?;
        let mut out = Vec::new();
        resp.body_mut()
            .as_reader()
            .take(limit + 1)
            .read_to_end(&mut out)
            .map_err(|e| NetError::from(ureq::Error::Io(e)))?;
        if out.len() as u64 > limit {
            return Err(NetError::TooLarge);
        }
        Ok(out)
    }

    /// `url`'yi `dest`'e indirir. Önce `dest.indiriliyor` dosyasına yazılır,
    /// bitince yerine taşınır; hata ya da iptalde geçici dosya silinir.
    /// `progress(indirilen, toplam)` `false` dönerse indirme iptal edilir.
    pub fn download(
        &self,
        url: &str,
        auth: Option<&Auth>,
        dest: &Path,
        limit: u64,
        mut progress: impl FnMut(u64, Option<u64>) -> bool,
    ) -> Result<u64, NetError> {
        let mut resp = self.request(url, auth)?;
        let total = resp
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok()?.parse::<u64>().ok());
        if total.is_some_and(|t| t > limit) {
            return Err(NetError::TooLarge);
        }
        let part = partial_path(dest);
        let result = (|| {
            let mut file = std::fs::File::create(&part).map_err(NetError::Io)?;
            let mut reader = resp.body_mut().as_reader();
            let mut buf = vec![0u8; 64 * 1024];
            let mut done = 0u64;
            loop {
                let n = reader
                    .read(&mut buf)
                    .map_err(|e| NetError::from(ureq::Error::Io(e)))?;
                if n == 0 {
                    break;
                }
                done += n as u64;
                if done > limit {
                    return Err(NetError::TooLarge);
                }
                file.write_all(&buf[..n]).map_err(NetError::Io)?;
                if !progress(done, total) {
                    return Err(NetError::Cancelled);
                }
            }
            if total.is_some_and(|t| t != done) {
                return Err(NetError::Offline("connection closed early".into()));
            }
            file.sync_all().map_err(NetError::Io)?;
            std::fs::rename(&part, dest).map_err(NetError::Io)?;
            Ok(done)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&part);
        }
        result
    }
}

pub fn partial_path(dest: &Path) -> PathBuf {
    PathBuf::from(format!("{}.{PARTIAL}", dest.display()))
}

/// Sunucunun istediği `Authorization` başlığı: MD5 Digest istendiyse o,
/// yöntem bildirilmediyse ya da Basic istendiyse Basic.
fn authorization(
    method: &str,
    challenges: &[String],
    auth: &Auth,
    url: &str,
) -> Result<String, NetError> {
    let digests: Vec<&str> = challenges
        .iter()
        .filter_map(|c| {
            let (scheme, rest) = c.trim().split_once(' ')?;
            scheme.eq_ignore_ascii_case("digest").then_some(rest)
        })
        .collect();
    if digests.is_empty() {
        let pair = format!("{}:{}", auth.user, auth.password);
        return Ok(format!("Basic {}", base64(pair.as_bytes())));
    }
    digests
        .iter()
        .find_map(|d| digest(method, &params(d), auth, &request_target(url), &cnonce()))
        .ok_or_else(|| NetError::Invalid(format!("unsupported digest challenge: {digests:?}")))
}

/// RFC 7616 Digest yanıtı (yalnızca MD5; `qop=auth` ya da eski biçim).
fn digest(
    method: &str,
    params: &[(String, String)],
    auth: &Auth,
    uri: &str,
    cnonce: &str,
) -> Option<String> {
    let get = |k: &str| {
        params
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.as_str())
    };
    let realm = get("realm")?;
    let nonce = get("nonce")?;
    if !get("algorithm").is_none_or(|a| a.eq_ignore_ascii_case("md5")) {
        return None;
    }
    let qop = match get("qop") {
        None => None,
        Some(q) if q.split(',').any(|t| t.trim() == "auth") => Some("auth"),
        Some(_) => return None, // yalnızca auth-int
    };
    let ha1 = md5_hex(&format!("{}:{realm}:{}", auth.user, auth.password));
    let ha2 = md5_hex(&format!("{method}:{uri}"));
    let nc = "00000001";
    let response = match qop {
        Some(q) => md5_hex(&format!("{ha1}:{nonce}:{nc}:{cnonce}:{q}:{ha2}")),
        None => md5_hex(&format!("{ha1}:{nonce}:{ha2}")),
    };
    let mut out = format!(
        "Digest username={}, realm={}, nonce={}, uri={}, response={}, algorithm=MD5",
        quote(&auth.user),
        quote(realm),
        quote(nonce),
        quote(uri),
        quote(&response)
    );
    if let Some(q) = qop {
        out.push_str(&format!(", qop={q}, nc={nc}, cnonce={}", quote(cnonce)));
    }
    if let Some(opaque) = get("opaque") {
        out.push_str(&format!(", opaque={}", quote(opaque)));
    }
    Some(out)
}

/// `anahtar=değer, anahtar="tırnaklı, değer"` listesi; anahtarlar küçük harfle.
fn params(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = s.trim();
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].trim().to_ascii_lowercase();
        rest = rest[eq + 1..].trim_start();
        let mut value = String::new();
        if let Some(quoted) = rest.strip_prefix('"') {
            let mut end = quoted.len();
            let mut escaped = false;
            for (i, c) in quoted.char_indices() {
                match c {
                    _ if escaped => {
                        value.push(c);
                        escaped = false;
                    }
                    '\\' => escaped = true,
                    '"' => {
                        end = i + 1;
                        break;
                    }
                    _ => value.push(c),
                }
            }
            rest = &quoted[end..];
        } else {
            let end = rest.find(',').unwrap_or(rest.len());
            value = rest[..end].trim().to_string();
            rest = &rest[end..];
        }
        rest = rest.trim_start().trim_start_matches(',').trim_start();
        out.push((key, value));
    }
    out
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// İstek satırındaki yol: `https://sunucu/a/b?c` → `/a/b?c`.
fn request_target(url: &str) -> String {
    let after_scheme = url.find("://").map_or(0, |i| i + 3);
    let url = url.split('#').next().unwrap_or(url);
    match url[after_scheme..].find(['/', '?']) {
        Some(i) if url[after_scheme + i..].starts_with('?') => {
            format!("/{}", &url[after_scheme + i..])
        }
        Some(i) => url[after_scheme + i..].to_string(),
        None => "/".to_string(),
    }
}

fn md5_hex(s: &str) -> String {
    use md5::{Digest, Md5};
    Md5::digest(s.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// İstemci rastgele sayısı: saat, süreç ve sayaçtan türetilir (gizli olması
/// gerekmez, yalnızca tekrarlanmamalı).
fn cnonce() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    md5_hex(&format!("{nanos}:{}:{n}", std::process::id()))[..16].to_string()
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in data.chunks(3) {
        let n = c
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod testserver {
    //! Testler için tek iş parçacıklı küçük HTTP sunucusu.
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// Her istekte `respond(yol, başlıklar)` çağrılır: (durum, gövde). Adresi döner.
    pub fn serve(respond: impl Fn(&str, &str) -> (u16, Vec<u8>) + Send + 'static) -> String {
        serve_with_headers(move |path, headers| {
            let (status, body) = respond(path, headers);
            (status, String::new(), body)
        })
    }

    /// `serve` gibi; yanıta eklenecek başlık satırları da (`"Ad: değer\r\n"`) döner.
    pub fn serve_with_headers(
        respond: impl Fn(&str, &str) -> (u16, String, Vec<u8>) + Send + 'static,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                if reader.read_line(&mut first).is_err() {
                    continue;
                }
                let path = first.split_whitespace().nth(1).unwrap_or("/").to_string();
                let mut headers = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    headers.push_str(&line);
                }
                let (status, extra, body) = respond(&path, &headers);
                let mut s = stream;
                let _ = write!(
                    s,
                    "HTTP/1.1 {status} X\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(&body);
            }
        });
        format!("http://{addr}")
    }
}

#[cfg(test)]
mod tests {
    use super::testserver::serve;
    use super::*;

    #[test]
    fn base64_matches_rfc4648() {
        for (plain, enc) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("kullanıcı:şifre", "a3VsbGFuxLFjxLE6xZ9pZnJl"),
        ] {
            assert_eq!(base64(plain.as_bytes()), enc);
        }
    }

    #[test]
    fn digest_matches_rfc2617_example() {
        let challenge = r#"Digest realm="testrealm@host.com", qop="auth,auth-int", nonce="dcd98b7102dd2f0e8b11d0f600bfb0c093", opaque="5ccc069c403ebaf9f0171e9517f40e41""#;
        let (_, rest) = challenge.split_once(' ').unwrap();
        let auth = Auth {
            user: "Mufasa".into(),
            password: "Circle Of Life".into(),
        };
        let header = digest("GET", &params(rest), &auth, "/dir/index.html", "0a4f113b").unwrap();
        assert!(
            header.contains(r#"response="6629fae49393a05397450978507c4ef1""#),
            "{header}"
        );
        assert!(header.contains("qop=auth, nc=00000001"));
        assert!(header.contains(r#"opaque="5ccc069c403ebaf9f0171e9517f40e41""#));
        // SHA-256 ya da yalnızca auth-int istenirse desteklenmez.
        let sha = params(r#"realm="r", nonce="n", algorithm=SHA-256"#);
        assert!(digest("GET", &sha, &auth, "/", "c").is_none());
        let int = params(r#"realm="r", nonce="n", qop="auth-int""#);
        assert!(digest("GET", &int, &auth, "/", "c").is_none());
    }

    #[test]
    fn challenge_parsing_and_request_targets() {
        assert_eq!(
            params(r#"realm="a, \"b\"", nonce=xyz ,qop="auth""#),
            [
                ("realm".to_string(), r#"a, "b""#.to_string()),
                ("nonce".to_string(), "xyz".to_string()),
                ("qop".to_string(), "auth".to_string()),
            ]
        );
        assert_eq!(
            request_target("http://h:8080/opds/nav?x=1#y"),
            "/opds/nav?x=1"
        );
        assert_eq!(request_target("http://h:8080"), "/");
        assert_eq!(request_target("http://h?q=1"), "/?q=1");
        assert_eq!(quote(r#"a"b\c"#), r#""a\"b\\c""#);
    }

    #[test]
    fn digest_challenge_is_answered_and_password_never_sent_first() {
        use std::sync::{Arc, Mutex};
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        // Calibre'nin düz HTTP'deki davranışı: MD5 Digest, qop=auth.
        let url = super::testserver::serve_with_headers(move |path, headers| {
            let header = headers.lines().find_map(|l| {
                let (name, value) = l.split_once(':')?;
                name.eq_ignore_ascii_case("authorization")
                    .then(|| value.trim().to_string())
            });
            log.lock().unwrap().push(header.clone());
            let challenge = "WWW-Authenticate: Digest realm=\"calibre\", nonce=\"n1\", algorithm=\"MD5\", qop=\"auth\"\r\n".to_string();
            let Some(h) = header else {
                return (401, challenge, Vec::new());
            };
            let p = params(h.strip_prefix("Digest ").unwrap_or(""));
            let get = |k: &str| {
                p.iter()
                    .find(|(key, _)| key == k)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default()
            };
            let ha1 = md5_hex("okur:calibre:gizli sifre");
            let ha2 = md5_hex(&format!("GET:{}", get("uri")));
            let expected = md5_hex(&format!(
                "{ha1}:n1:{}:{}:auth:{ha2}",
                get("nc"),
                get("cnonce")
            ));
            if get("uri") == path && get("response") == expected {
                (200, String::new(), b"katalog".to_vec())
            } else {
                (401, challenge, Vec::new())
            }
        });
        let net = Net::default();
        let auth = Auth {
            user: "okur".into(),
            password: "gizli sifre".into(),
        };
        let body = net
            .get(&format!("{url}/opds?library_id=lib"), Some(&auth), 100)
            .unwrap();
        assert_eq!(body, b"katalog");
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0], None, "şifre ilk istekte gönderilmez");
        assert!(seen[1].as_deref().is_some_and(|h| h.starts_with("Digest ")));
        // Yanlış şifre: Basic'e düşülmez, yetki hatası.
        let wrong = Auth {
            password: "yanlis".into(),
            ..auth
        };
        assert!(matches!(
            net.get(&format!("{url}/opds"), Some(&wrong), 100),
            Err(NetError::Unauthorized)
        ));
    }

    #[test]
    fn statuses_and_auth() {
        let url = serve(|path, headers| match path {
            "/gizli"
                if headers.lines().any(|l| {
                    l.to_ascii_lowercase().starts_with("authorization:")
                        && l.trim_end().ends_with("Basic YTpi")
                }) =>
            {
                (200, b"tamam".to_vec())
            }
            "/gizli" => (401, Vec::new()),
            "/yok" => (404, Vec::new()),
            _ => (200, b"merhaba".to_vec()),
        });
        let net = Net::default();
        assert_eq!(net.get(&format!("{url}/"), None, 100).unwrap(), b"merhaba");
        assert!(matches!(
            net.get(&format!("{url}/gizli"), None, 100),
            Err(NetError::Unauthorized)
        ));
        let auth = Auth {
            user: "a".into(),
            password: "b".into(),
        };
        assert_eq!(
            net.get(&format!("{url}/gizli"), Some(&auth), 100).unwrap(),
            b"tamam"
        );
        assert!(matches!(
            net.get(&format!("{url}/yok"), None, 100),
            Err(NetError::Status(404))
        ));
        assert!(matches!(
            net.get(&format!("{url}/"), None, 3),
            Err(NetError::TooLarge)
        ));
    }

    #[test]
    fn unreachable_and_slow_servers_fail_cleanly() {
        // Kapalı port: bağlantı reddedilir.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let net = Net::with_timeouts(Duration::from_millis(500), Duration::from_millis(300));
        let e = net
            .get(&format!("http://127.0.0.1:{port}/"), None, 10)
            .unwrap_err();
        assert!(matches!(e, NetError::Offline(_)), "{e:?}");
        // Yanıt vermeyen sunucu: zaman aşımı.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let _held: Vec<_> = listener.incoming().take(1).collect();
            std::thread::sleep(Duration::from_secs(3));
        });
        let e = net.get(&format!("http://{addr}/"), None, 10).unwrap_err();
        assert!(matches!(e, NetError::Timeout), "{e:?}");
        assert_eq!(e.message(), Msg::NetTimeout);
    }

    #[test]
    fn downloads_report_progress_and_cancel_without_leftovers() {
        let body: Vec<u8> = (0..300_000u32).map(|i| i as u8).collect();
        let served = body.clone();
        let url = serve(move |_, _| (200, served.clone()));
        let dir = std::env::temp_dir().join(format!("legentia-net-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("kitap.epub");
        let net = Net::default();
        let mut seen = Vec::new();
        let n = net
            .download(&url, None, &dest, 1 << 20, |done, total| {
                seen.push((done, total));
                true
            })
            .unwrap();
        assert_eq!(n, 300_000);
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert_eq!(seen.last(), Some(&(300_000, Some(300_000))));
        assert!(!partial_path(&dest).exists());

        let dest2 = dir.join("iptal.epub");
        let e = net
            .download(&url, None, &dest2, 1 << 20, |done, _| done < 100_000)
            .unwrap_err();
        assert!(matches!(e, NetError::Cancelled));
        assert!(
            !dest2.exists() && !partial_path(&dest2).exists(),
            "iptalde dosya kalmaz"
        );
        let e = net
            .download(&url, None, &dest2, 1000, |_, _| true)
            .unwrap_err();
        assert!(matches!(e, NetError::TooLarge));
        assert!(!partial_path(&dest2).exists());
    }
}
