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
            .user_agent(USER_AGENT)
            .build();
        Net {
            agent: ureq::Agent::new_with_config(config),
        }
    }

    fn request(
        &self,
        url: &str,
        auth: Option<&Auth>,
    ) -> Result<ureq::http::Response<ureq::Body>, NetError> {
        let mut req = self.agent.get(url);
        if let Some(a) = auth {
            req = req.header(
                "Authorization",
                format!(
                    "Basic {}",
                    base64(format!("{}:{}", a.user, a.password).as_bytes())
                ),
            );
        }
        let resp = req.call()?;
        match resp.status().as_u16() {
            200..=299 => Ok(resp),
            401 | 403 => Err(NetError::Unauthorized),
            code => Err(NetError::Status(code)),
        }
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
                let (status, body) = respond(&path, &headers);
                let mut s = stream;
                let _ = write!(
                    s,
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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
