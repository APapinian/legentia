//! Uyku: Kobo çekirdeğinin askıya alma yolu (KOReader'ın izlediği sıra):
//! `state-extended`'e 1, kısa bekleme, `sync`, `state`'e `mem` (uyanana kadar
//! bekler), sonra `state-extended`'e 0. Wi-Fi açıksa önce `wifi.sh kapat`
//! (sürücüsü askıya almayı bozar), uyanınca arka planda yeniden açılır;
//! kapatılamazsa çekirdek uyutulmaz, yalnız ekran ve ışık kapanır.

use std::time::Duration;

const STATE: &str = "/sys/power/state";
const STATE_EXTENDED: &str = "/sys/power/state-extended";
/// Kobo'larda Wi-Fi arabirimi modele göre eth0 ya da wlan0.
const WIFI: [&str; 2] = [
    "/sys/class/net/eth0/operstate",
    "/sys/class/net/wlan0/operstate",
];

fn write(path: &str, value: &str) -> bool {
    match std::fs::write(path, value) {
        Ok(()) => true,
        Err(e) => {
            log::warn!("cannot write {value:?} to {path}: {e}");
            false
        }
    }
}

pub fn wifi_up() -> bool {
    WIFI.iter()
        .any(|p| std::fs::read_to_string(p).is_ok_and(|s| s.trim() == "up"))
}

/// Programın yanındaki `wifi.sh` (`ac` ya da `kapat`); bitmesini bekler.
pub fn wifi(action: &str) -> bool {
    let script = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("wifi.sh")))
        .unwrap_or_else(|| "wifi.sh".into());
    match std::process::Command::new("sh")
        .arg(&script)
        .arg(action)
        .status()
    {
        Ok(s) => s.success(),
        Err(e) => {
            log::warn!("cannot run {}: {e}", script.display());
            false
        }
    }
}

/// Çekirdeği askıya alır; uyanınca döner. Wi-Fi açıksa önce kapatılır,
/// uyanınca arka planda yeniden açılır. Askıya alınamadıysa `false`
/// (çağıran güç tuşunu kendisi bekler).
pub fn suspend() -> bool {
    let had_wifi = wifi_up();
    if had_wifi {
        wifi("kapat");
        if wifi_up() {
            log::info!("sleep: wifi could not be turned off, not suspending the kernel");
            return false;
        }
    }
    if !write(STATE_EXTENDED, "1") {
        return false;
    }
    std::thread::sleep(Duration::from_secs(2));
    let _ = std::process::Command::new("sync").status();
    log::info!("sleep: suspending");
    let ok = write(STATE, "mem");
    write(STATE_EXTENDED, "0");
    log::info!("sleep: woke up (suspended: {ok})");
    if had_wifi {
        std::thread::spawn(|| wifi("ac"));
    }
    ok
}
