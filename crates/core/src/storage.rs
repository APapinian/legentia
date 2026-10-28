//! Kullanıcı verisinin yerel kaydı: sürüm numaralı JSON dosyaları.
//!
//! ```text
//! <veri klasörü>/settings.json      okuma ayarları
//! <veri klasörü>/library.json       kütüphane önbelleği (kitap başlıkları)
//! <veri klasörü>/books/<kimlik>.json  kitap başına konum, yer imleri, vurgular ve notlar
//! ```
//!
//! Yazma önce `.tmp` dosyasına yapılır, sonra yeniden adlandırılır; yarıda
//! kesilen yazma eski dosyayı bozmaz. Okunamayan ya da daha yeni sürümlü bir
//! dosyanın üzerine sessizce yazılmaz: `.bozuk` uzantısıyla kenara alınır.
//!
//! Göç (migration): her dosya `version` alanı taşır. Biçim değişince sürüm
//! artırılır ve `load` içinde eski sürümden dönüştürme eklenir.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Bu yazılımın yazdığı dosya biçimi sürümü.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    version: u32,
    data: T,
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// Dosya yoksa ya da okunamıyorsa `None`. Bozuk dosya kenara alınır.
    pub(crate) fn load<T: DeserializeOwned>(&self, name: &str) -> Option<T> {
        let path = self.path(name);
        let bytes = std::fs::read(&path).ok()?;
        match serde_json::from_slice::<Envelope<T>>(&bytes) {
            Ok(env) if env.version == FORMAT_VERSION => Some(env.data),
            Ok(env) => {
                log::warn!("{} has unsupported version {}", path.display(), env.version);
                self.quarantine(&path);
                None
            }
            Err(e) => {
                log::warn!("{} is unreadable: {e}", path.display());
                self.quarantine(&path);
                None
            }
        }
    }

    /// Okunamayan dosyayı silmeden kenara alır: `ad.bozuk`, varsa `ad.bozuk.2`…
    fn quarantine(&self, path: &Path) {
        for n in 1.. {
            let suffix = if n == 1 {
                ".bozuk".to_string()
            } else {
                format!(".bozuk.{n}")
            };
            let target = PathBuf::from(format!("{}{suffix}", path.display()));
            if !target.exists() {
                if let Err(e) = std::fs::rename(path, &target) {
                    log::error!("cannot move {} aside: {e}", path.display());
                }
                return;
            }
        }
    }

    pub(crate) fn save<T: Serialize>(&self, name: &str, data: &T) {
        if let Err(e) = self.try_save(name, data) {
            log::error!("cannot save {name}: {e}");
        }
    }

    fn try_save<T: Serialize>(&self, name: &str, data: &T) -> std::io::Result<()> {
        use std::io::Write;
        let path = self.path(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(&Envelope {
            version: FORMAT_VERSION,
            data,
        })?;
        let tmp = PathBuf::from(format!("{}.tmp", path.display()));
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(&json)?;
        file.sync_all()?;
        std::fs::rename(&tmp, &path)
    }
}

#[cfg(test)]
pub(crate) fn temp_store(tag: &str) -> Store {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "legentia-test-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    Store::new(&dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Sample {
        word: String,
    }

    #[test]
    fn round_trips_turkish_text() {
        let store = temp_store("roundtrip");
        store.save(
            "books/a.json",
            &Sample {
                word: "Ağaç gölgesi".into(),
            },
        );
        assert_eq!(
            store.load::<Sample>("books/a.json"),
            Some(Sample {
                word: "Ağaç gölgesi".into()
            })
        );
        assert!(!store.dir().join("books/a.json.tmp").exists());
    }

    #[test]
    fn corrupt_file_is_moved_aside_not_overwritten() {
        let store = temp_store("corrupt");
        std::fs::create_dir_all(store.dir()).unwrap();
        std::fs::write(store.dir().join("settings.json"), b"{ bozuk").unwrap();
        assert_eq!(store.load::<Sample>("settings.json"), None);
        assert_eq!(
            std::fs::read(store.dir().join("settings.json.bozuk")).unwrap(),
            b"{ bozuk"
        );
    }

    #[test]
    fn newer_version_is_not_silently_overwritten() {
        let store = temp_store("newer");
        std::fs::create_dir_all(store.dir()).unwrap();
        let future = br#"{"version": 99, "data": {"word": "x"}}"#;
        std::fs::write(store.dir().join("s.json"), future).unwrap();
        assert_eq!(store.load::<Sample>("s.json"), None);
        assert!(store.dir().join("s.json.bozuk").exists());
    }

    #[test]
    fn missing_file_is_none() {
        assert_eq!(temp_store("missing").load::<Sample>("yok.json"), None);
    }
}
