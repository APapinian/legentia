//! Cihaz durumu ve ışık: pil ile saat alt bilgi satırının sağında; ön ışık
//! (parlaklık ve sıcaklık) okuma menüsündeki "Işık" ekranından. Değerleri
//! platform verir ve uygular (`set_device_status`, `light`); çekirdek dosyaya
//! dokunmaz. Saat UTC gelir, ayardaki saat dilimiyle gösterilir.

use super::ui::{BOLD, HEADER_BOTTOM, MARGIN, ROW_PX, Rect, back_button, draw_button, draw_header};
use super::{App, View};
use crate::canvas::Canvas;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};
use crate::text::Family;

/// Platformun verdiği anlık durum.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeviceStatus {
    /// Pil yüzdesi; bilinmiyorsa (masaüstü) yok.
    pub battery: Option<u8>,
    pub charging: bool,
    /// Şimdiki zaman (Unix saniyesi, UTC).
    pub unix_time: Option<u64>,
}

/// Parlaklık adımları (yüzde): karanlıkta ince ayar için düşük uçta sık.
const BRIGHTNESS: [u8; 18] = [
    0, 1, 2, 3, 5, 8, 10, 15, 20, 25, 30, 40, 50, 60, 70, 80, 90, 100,
];
pub(super) const MAX_WARMTH: u8 = 10;
const STEP: i32 = 110;

impl App {
    /// Platform: pil ve saat (her olaydan önce çağrılabilir; ucuzdur).
    pub fn set_device_status(&mut self, status: DeviceStatus) {
        self.status = status;
    }

    /// Platform: uygulanacak ışık (parlaklık 0–100, sıcaklık 0–10); kullanıcı
    /// hiç ayarlamadıysa `None` (cihazın kendi ayarı kalır).
    pub fn light(&self) -> Option<(u8, u8)> {
        let b = self.settings.light?;
        Some((b, self.settings.warmth.unwrap_or(MAX_WARMTH / 2)))
    }

    /// "15:42 · %87" (şarjdaysa "şarj" eki); bilinmeyen kısım yazılmaz.
    pub(super) fn status_text(&self) -> Option<String> {
        let s = self.status;
        let clock = s.unix_time.map(|t| {
            let local = t as i64 + self.settings.utc_offset as i64 * 60;
            let minutes = local.rem_euclid(86_400) / 60;
            format!("{:02}:{:02}", minutes / 60, minutes % 60)
        });
        let battery = s.battery.map(|b| {
            tr(&if s.charging {
                Msg::BatteryCharging(b)
            } else {
                Msg::Battery(b)
            })
        });
        let parts: Vec<String> = clock.into_iter().chain(battery).collect();
        (!parts.is_empty()).then(|| parts.join(" · "))
    }

    /// Durum, alt bilgi satırının sağında; soldaki yazı `left_end`'e kadar
    /// uzanıyorsa ve sığmıyorsa yazılmaz.
    pub(super) fn draw_status(&self, canvas: &mut Canvas, left_end: i32) {
        let Some(text) = self.status_text() else {
            return;
        };
        let t = &self.text;
        let px = super::ui::FOOTER_PX;
        let w = t.width_plain(&text, Family::Ui, super::ui::PLAIN, px) as i32;
        let x = canvas.width() as i32 - MARGIN.min(self.settings.margin) - w;
        if x < left_end + 30 {
            return;
        }
        let baseline =
            canvas.height() as i32 - (crate::LIBRA2_HEIGHT as i32 - super::ui::FOOTER_BASELINE);
        t.draw_plain(canvas, &text, Family::Ui, super::ui::PLAIN, x, baseline, px);
    }

    // --- Işık ekranı ---

    /// Satırın (0 parlaklık, 1 sıcaklık) azaltma ve artırma düğmeleri.
    pub(super) fn light_buttons(&self, row: i32) -> (Rect, Rect) {
        let y = HEADER_BOTTOM + 120 + row * 220;
        let right = self.width as i32 - MARGIN;
        (
            Rect {
                x: right - 2 * STEP - 30,
                y,
                w: STEP,
                h: STEP,
            },
            Rect {
                x: right - STEP,
                y,
                w: STEP,
                h: STEP,
            },
        )
    }

    pub(super) fn light_off_button(&self) -> Rect {
        Rect {
            x: MARGIN,
            y: HEADER_BOTTOM + 120 + 2 * 220,
            w: self.width as i32 - 2 * MARGIN,
            h: STEP,
        }
    }

    pub(super) fn handle_light(&mut self, event: Event) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        if back_button(self.width).contains(x, y) {
            self.view = View::Reading;
            return Some(self.refresh(true));
        }
        let mut next = self.settings.clone();
        let current = next.light.unwrap_or(0);
        let i = BRIGHTNESS
            .iter()
            .position(|&b| b >= current)
            .unwrap_or(BRIGHTNESS.len() - 1);
        let warmth = next.warmth.unwrap_or(MAX_WARMTH / 2);
        let (minus, plus) = self.light_buttons(0);
        let (cooler, warmer) = self.light_buttons(1);
        if minus.contains(x, y) {
            next.light = Some(BRIGHTNESS[i.saturating_sub(1)]);
        } else if plus.contains(x, y) {
            next.light = Some(BRIGHTNESS[(i + 1).min(BRIGHTNESS.len() - 1)]);
        } else if cooler.contains(x, y) {
            next.warmth = Some(warmth.saturating_sub(1));
            next.light.get_or_insert(current);
        } else if warmer.contains(x, y) {
            next.warmth = Some((warmth + 1).min(MAX_WARMTH));
            next.light.get_or_insert(current);
        } else if self.light_off_button().contains(x, y) {
            next.light = Some(0);
        } else {
            return None;
        }
        self.apply_settings(next);
        Some(self.refresh(false))
    }

    pub(super) fn draw_light(&self, canvas: &mut Canvas) {
        let t = &self.text;
        draw_header(t, canvas, &tr(&Msg::Light), &tr(&Msg::LightHint), true);
        draw_button(
            t,
            canvas,
            back_button(canvas.width()),
            &tr(&Msg::Ok),
            ROW_PX,
        );
        let rows = [
            (
                Msg::Brightness,
                self.settings
                    .light
                    .map_or_else(|| "–".to_string(), |b| tr(&Msg::Percent(b as u32))),
            ),
            (
                Msg::Warmth,
                format!(
                    "{}/{MAX_WARMTH}",
                    self.settings.warmth.unwrap_or(MAX_WARMTH / 2)
                ),
            ),
        ];
        for (row, (label, value)) in rows.iter().enumerate() {
            let (minus, plus) = self.light_buttons(row as i32);
            let baseline = minus.y + STEP / 2 + 14;
            t.draw_msg(canvas, label, MARGIN, baseline, ROW_PX);
            let vx = minus.x - 40 - t.width_plain(value, Family::Ui, BOLD, ROW_PX) as i32;
            t.draw_plain(canvas, value, Family::Ui, BOLD, vx, baseline, ROW_PX);
            draw_button(t, canvas, minus, "−", 48.0);
            draw_button(t, canvas, plus, "+", 48.0);
        }
        draw_button(
            t,
            canvas,
            self.light_off_button(),
            &tr(&Msg::LightOff),
            ROW_PX,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_shows_local_time_and_battery() {
        let text = crate::text::test_text();
        let store = crate::storage::temp_store("durum");
        let dir = store.dir().to_path_buf();
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = App::new(text, crate::LIBRA2_WIDTH, store, &dir);
        assert_eq!(app.status_text(), None);
        // 2026-10-29 12:59 UTC → Türkiye (UTC+3) 15:59.
        app.set_device_status(DeviceStatus {
            battery: Some(87),
            charging: false,
            unix_time: Some(1_793_278_762),
        });
        assert_eq!(app.status_text().as_deref(), Some("15:59 · %87"));
        app.settings.utc_offset = -330;
        app.status.charging = true;
        assert_eq!(app.status_text().as_deref(), Some("07:29 · %87 şarj"));
    }
}
