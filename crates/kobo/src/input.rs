//! Girdi: `/dev/input/event*` aygıtları (evdev). Dokunmatik ekran (Elan,
//! adında "touch" geçen aygıt) ham dokunmaya, sayfa tuşları ve güç tuşu
//! komuta çevrilir. Her aygıt kendi iş parçacığında okunur, olaylar tek
//! kanala akar. Diğer aygıtlardan (ör. ivmeölçer) yalnızca tuşlar alınır.

use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;

use legentia_core::{LIBRA2_HEIGHT, LIBRA2_WIDTH, PageKey, TouchPhase};

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const EV_ABS: u16 = 3;
const SYN_REPORT: u16 = 0;
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;
const BTN_TOUCH: u16 = 0x14a;
const KEY_POWER: u16 = 116;
/// Libra 2 sayfa tuşları: üst (geri) ve alt (ileri).
const KEY_PAGE_BACK: u16 = 193;
const KEY_PAGE_FORWARD: u16 = 194;

/// Dokunmatiğin ham aralıkları (Cobalt'ın Libra 2 ölçümü).
const RAW_X_MAX: i64 = 1680;
const RAW_Y_MAX: i64 = 1264;

/// `struct input_event`: zaman damgası (iki `long`), tür, kod, değer.
pub const EVENT_SIZE: usize = if cfg!(target_pointer_width = "64") {
    24
} else {
    16
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Touch {
        phase: TouchPhase,
        x: u32,
        y: u32,
    },
    /// İkinci parmak (iki parmak hareketi).
    SecondTouch {
        phase: TouchPhase,
        x: u32,
        y: u32,
    },
    Page(PageKey),
    Power,
    /// İvmeölçer: cihaz yatay mı ve baskın eksenin yönü (yatayın hangi yana olduğu).
    Tilt {
        landscape: bool,
        positive: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawEvent {
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

pub fn parse(buf: &[u8; EVENT_SIZE]) -> RawEvent {
    let tail = &buf[EVENT_SIZE - 8..];
    RawEvent {
        kind: u16::from_ne_bytes([tail[0], tail[1]]),
        code: u16::from_ne_bytes([tail[2], tail[3]]),
        value: i32::from_ne_bytes([tail[4], tail[5], tail[6], tail[7]]),
    }
}

/// Ham dokunma konumunu framebuffer dönüşüne (`rotate`) göre ekran pikseline
/// çevirir. 1 (dikey) Cobalt'ın ölçümü: eksenler yer değiştirir. 3 (ters
/// dikey) cihazda görüldü: ayrıca iki eksen aynalanır. 0 ve 2 (yatay)
/// geometriden çıkarıldı, cihazda doğrulanmadı.
pub fn to_screen(raw_x: i32, raw_y: i32, rotate: u32) -> (u32, u32) {
    let scale = |v: i32, max: i64, pixels: u32| {
        let v = (v as i64).clamp(0, max);
        ((v * (pixels as i64 - 1) + max / 2) / max) as u32
    };
    // Kısa kenar boyunca (sx) ve uzun kenar boyunca (sy) konum.
    let sx = scale(raw_y, RAW_Y_MAX, LIBRA2_WIDTH);
    let sy = scale(raw_x, RAW_X_MAX, LIBRA2_HEIGHT);
    let (w, h) = (LIBRA2_WIDTH - 1, LIBRA2_HEIGHT - 1);
    match rotate % 4 {
        1 => (sx, sy),
        3 => (w - sx, h - sy),
        2 => (h - sy, sx),
        _ => (sy, w - sx),
    }
}

/// İki parmak izleyen dokunma durumu (yuva 0 ve 1; fazlası yok sayılır).
/// Hem çok noktalı (B protokolü) hem tek noktalı (`ABS_X`, `BTN_TOUCH`)
/// bildirimleri anlar; tek noktalıda yalnız ilk parmak vardır.
#[derive(Debug)]
pub struct Touch {
    /// Framebuffer'ın geçerli dönüşü (okuma yataya dönünce değişir).
    rotation: Arc<AtomicU32>,
    slot: usize,
    fingers: [Finger; 2],
}

#[derive(Debug, Default, Clone, Copy)]
struct Finger {
    down: bool,
    was_down: bool,
    raw: (i32, i32),
    sent: Option<(u32, u32)>,
}

impl Finger {
    fn report(&mut self, rotate: u32) -> Option<(TouchPhase, u32, u32)> {
        let (x, y) = to_screen(self.raw.0, self.raw.1, rotate);
        let phase = match (self.was_down, self.down) {
            (false, true) => TouchPhase::Down,
            (true, true) if self.sent != Some((x, y)) => TouchPhase::Move,
            (true, false) => TouchPhase::Up,
            _ => return None,
        };
        self.was_down = self.down;
        self.sent = self.down.then_some((x, y));
        Some((phase, x, y))
    }
}

impl Touch {
    pub fn new(rotation: Arc<AtomicU32>) -> Touch {
        Touch {
            rotation,
            slot: 0,
            fingers: Default::default(),
        }
    }

    pub fn feed(&mut self, e: RawEvent) -> Vec<Input> {
        match (e.kind, e.code) {
            (EV_ABS, ABS_MT_SLOT) => self.slot = e.value.max(0) as usize,
            (EV_ABS, code) if code >= ABS_MT_SLOT && self.slot > 1 => {}
            (EV_ABS, ABS_MT_TRACKING_ID) => self.fingers[self.slot].down = e.value >= 0,
            (EV_KEY, BTN_TOUCH) => self.fingers[0].down = e.value != 0,
            (EV_ABS, ABS_MT_POSITION_X) => self.fingers[self.slot].raw.0 = e.value,
            (EV_ABS, ABS_MT_POSITION_Y) => self.fingers[self.slot].raw.1 = e.value,
            (EV_ABS, ABS_X) => self.fingers[0].raw.0 = e.value,
            (EV_ABS, ABS_Y) => self.fingers[0].raw.1 = e.value,
            (EV_SYN, SYN_REPORT) => return self.report(),
            _ => {}
        }
        Vec::new()
    }

    fn report(&mut self) -> Vec<Input> {
        let rotate = self.rotation.load(Ordering::Relaxed);
        let first = self.fingers[0]
            .report(rotate)
            .map(|(phase, x, y)| Input::Touch { phase, x, y });
        let second = self.fingers[1]
            .report(rotate)
            .map(|(phase, x, y)| Input::SecondTouch { phase, x, y });
        first.into_iter().chain(second).collect()
    }
}

/// Basılan tuşun komutu (tekrar ve bırakma yok sayılır). Ters dikeyde (3)
/// fiziksel üst tuş 194 olur; üstteki tuş her zaman geri. Yatayda (0, 2)
/// tuşların anlamı cihazda doğrulanmadı; 2'de de yer değiştirir.
pub fn key(e: RawEvent, rotate: u32) -> Option<Input> {
    if e.kind != EV_KEY || e.value != 1 {
        return None;
    }
    let (back, forward) = if matches!(rotate % 4, 2 | 3) {
        (KEY_PAGE_FORWARD, KEY_PAGE_BACK)
    } else {
        (KEY_PAGE_BACK, KEY_PAGE_FORWARD)
    };
    match e.code {
        c if c == back => Some(Input::Page(PageKey::Back)),
        c if c == forward => Some(Input::Page(PageKey::Forward)),
        KEY_POWER => Some(Input::Power),
        _ => None,
    }
}

/// Baskın eksen öbürünün en az bu katıysa eğim sayılır (aradaki açılar yok sayılır).
const DOMINANCE: f32 = 1.5;
/// Yeni yön bu kadar ardışık okumada aynı kalınca bildirilir.
const STABLE_READINGS: u8 = 3;

/// İvmeölçer (Libra 2'de kx122): yerçekiminin baskın ekseni. Eksenlerin cihaza
/// göre yönü bilinmediği için açılıştaki baskın eksen dikey sayılır (program
/// Nickel'dan hep dikey ekranda başlatılır); öbür eksen baskınlaşınca yatay.
#[derive(Debug, Default)]
pub struct Accel {
    axes: [i32; 3],
    portrait_axis: Option<usize>,
    reported: Option<(bool, bool)>,
    candidate: Option<((bool, bool), u8)>,
}

impl Accel {
    pub fn feed(&mut self, e: RawEvent) -> Option<Input> {
        match (e.kind, e.code) {
            (EV_ABS, code @ 0..=2) => self.axes[code as usize] = e.value,
            (EV_SYN, SYN_REPORT) => return self.report(),
            _ => {}
        }
        None
    }

    fn report(&mut self) -> Option<Input> {
        let axes = self.axes;
        let (x, y) = (axes[0].abs() as f32, axes[1].abs() as f32);
        let axis = if x > y * DOMINANCE {
            0
        } else if y > x * DOMINANCE {
            1
        } else {
            return None;
        };
        let portrait = *self.portrait_axis.get_or_insert_with(|| {
            log::info!("accelerometer: portrait axis {axis}, reading {axes:?}");
            axis
        });
        let state = (axis != portrait, axes[axis] > 0);
        let count = match self.candidate {
            Some((s, n)) if s == state => n.saturating_add(1),
            _ => 1,
        };
        self.candidate = Some((state, count));
        if count < STABLE_READINGS || self.reported == Some(state) {
            return None;
        }
        // Açılıştaki ilk kararlı okuma dikeydir; bildirmeye gerek yok.
        let first = self.reported.is_none();
        self.reported = Some(state);
        log::info!(
            "accelerometer: landscape {}, positive {} ({axes:?})",
            state.0,
            state.1
        );
        (!first || state.0).then_some(Input::Tilt {
            landscape: state.0,
            positive: state.1,
        })
    }
}

/// Bütün girdi aygıtlarını ayrı iş parçacıklarında okur; okunamayan aygıt
/// günlüğe yazılıp atlanır. Açılan aygıt sayısını döner.
pub fn spawn_readers(tx: &Sender<Input>, rotation: &Arc<AtomicU32>) -> usize {
    let mut count = 0;
    let Ok(entries) = std::fs::read_dir("/dev/input") else {
        log::error!("cannot list /dev/input");
        return 0;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.starts_with("event") {
            continue;
        }
        let device = std::fs::read_to_string(
            Path::new("/sys/class/input")
                .join(&name)
                .join("device/name"),
        )
        .unwrap_or_default()
        .trim()
        .to_string();
        let lower = device.to_lowercase();
        let (touch, accel) = (lower.contains("touch"), lower.contains("accel"));
        let mut file = match std::fs::File::open(e.path()) {
            Ok(f) => f,
            Err(err) => {
                log::warn!("cannot open {name} ({device}): {err}");
                continue;
            }
        };
        log::info!("input {name}: {device} (touch: {touch}, accelerometer: {accel})");
        let rotation = rotation.clone();
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut state = Touch::new(rotation.clone());
            let mut tilt = Accel::default();
            let mut buf = [0u8; EVENT_SIZE];
            while file.read_exact(&mut buf).is_ok() {
                let raw = parse(&buf);
                let mut inputs = if touch { state.feed(raw) } else { Vec::new() };
                if accel {
                    inputs.extend(tilt.feed(raw));
                } else {
                    inputs.extend(key(raw, rotation.load(Ordering::Relaxed)));
                }
                if inputs.into_iter().any(|i| tx.send(i).is_err()) {
                    return;
                }
            }
            log::warn!("input {name} closed");
        });
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: u16, code: u16, value: i32) -> RawEvent {
        RawEvent { kind, code, value }
    }

    #[test]
    fn events_are_parsed_from_the_kernel_layout() {
        let mut buf = [0u8; EVENT_SIZE];
        buf[EVENT_SIZE - 8..EVENT_SIZE - 6].copy_from_slice(&EV_KEY.to_ne_bytes());
        buf[EVENT_SIZE - 6..EVENT_SIZE - 4].copy_from_slice(&KEY_PAGE_FORWARD.to_ne_bytes());
        buf[EVENT_SIZE - 4..].copy_from_slice(&1i32.to_ne_bytes());
        assert_eq!(parse(&buf), ev(EV_KEY, KEY_PAGE_FORWARD, 1));
        assert_eq!(key(parse(&buf), 1), Some(Input::Page(PageKey::Forward)));
        assert_eq!(
            key(ev(EV_KEY, KEY_PAGE_FORWARD, 2), 1),
            None,
            "tekrar yok sayılır"
        );
        assert_eq!(key(ev(EV_KEY, KEY_POWER, 1), 1), Some(Input::Power));
    }

    #[test]
    fn axes_are_swapped_and_scaled() {
        assert_eq!(to_screen(0, 0, 1), (0, 0));
        assert_eq!(
            to_screen(1680, 1264, 1),
            (LIBRA2_WIDTH - 1, LIBRA2_HEIGHT - 1)
        );
        // Ham x ekranın dikeyi, ham y yatayı.
        assert_eq!(to_screen(840, 0, 1), (0, 840));
        assert_eq!(to_screen(0, 632, 1), (632, 0));
        assert_eq!(
            to_screen(-5, 99_999, 1),
            (LIBRA2_WIDTH - 1, 0),
            "aralık dışı kırpılır"
        );
    }

    #[test]
    fn upside_down_screen_mirrors_touches_and_swaps_page_keys() {
        assert_eq!(to_screen(0, 0, 3), (LIBRA2_WIDTH - 1, LIBRA2_HEIGHT - 1));
        assert_eq!(to_screen(1680, 1264, 3), (0, 0));
        let (x, y) = to_screen(840, 300, 1);
        assert_eq!(
            to_screen(840, 300, 3),
            (LIBRA2_WIDTH - 1 - x, LIBRA2_HEIGHT - 1 - y)
        );
        assert_eq!(
            key(ev(EV_KEY, KEY_PAGE_BACK, 1), 3),
            Some(Input::Page(PageKey::Forward))
        );
        assert_eq!(
            key(ev(EV_KEY, KEY_PAGE_FORWARD, 1), 3),
            Some(Input::Page(PageKey::Back))
        );
        assert_eq!(key(ev(EV_KEY, KEY_POWER, 1), 3), Some(Input::Power));
    }

    #[test]
    fn landscape_rotations_stay_inside_the_wide_screen() {
        // Yatay ekran 1680×1264; dört köşe ekranın dört köşesine düşer.
        for rotate in [0, 2] {
            let corners: Vec<(u32, u32)> = [(0, 0), (1680, 0), (0, 1264), (1680, 1264)]
                .into_iter()
                .map(|(x, y)| to_screen(x, y, rotate))
                .collect();
            for (x, y) in &corners {
                assert!(
                    *x < LIBRA2_HEIGHT && *y < LIBRA2_WIDTH,
                    "{rotate}: {corners:?}"
                );
            }
            let mut sorted = corners.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), 4, "{rotate}: {corners:?}");
        }
        // 0 ile 2 birbirinin 180° döndürülmüşü.
        let (x0, y0) = to_screen(300, 200, 0);
        let (x2, y2) = to_screen(300, 200, 2);
        assert_eq!((x0 + x2, y0 + y2), (LIBRA2_HEIGHT - 1, LIBRA2_WIDTH - 1));
    }

    #[test]
    fn accelerometer_calibrates_on_the_first_reading_and_needs_stable_tilts() {
        let mut a = Accel::default();
        let reading = |a: &mut Accel, x: i32, y: i32| {
            a.feed(ev(EV_ABS, 0, x));
            a.feed(ev(EV_ABS, 1, y));
            (0..3)
                .filter_map(|_| a.feed(ev(EV_SYN, SYN_REPORT, 0)))
                .last()
        };
        // Açılış: dikey tutuluyor (y baskın); bildirilmez.
        assert_eq!(reading(&mut a, 50, -1000), None);
        // Çapraz (baskın eksen yok): yok sayılır.
        assert_eq!(reading(&mut a, 700, -700), None);
        // Yatay: x baskın.
        assert_eq!(
            reading(&mut a, -1000, 30),
            Some(Input::Tilt {
                landscape: true,
                positive: false
            })
        );
        assert_eq!(
            reading(&mut a, -990, 40),
            None,
            "aynı yön yeniden bildirilmez"
        );
        assert_eq!(
            reading(&mut a, 20, -1000),
            Some(Input::Tilt {
                landscape: false,
                positive: false
            })
        );
    }

    #[test]
    fn multitouch_contact_becomes_down_move_up() {
        let mut t = Touch::new(Arc::new(AtomicU32::new(1)));
        let mut out = Vec::new();
        for e in [
            ev(EV_ABS, ABS_MT_TRACKING_ID, 7),
            ev(EV_ABS, ABS_MT_POSITION_X, 100),
            ev(EV_ABS, ABS_MT_POSITION_Y, 200),
            ev(EV_SYN, SYN_REPORT, 0),
            ev(EV_SYN, SYN_REPORT, 0), // değişiklik yok: olay yok
            ev(EV_ABS, ABS_MT_POSITION_X, 300),
            ev(EV_SYN, SYN_REPORT, 0),
            // İkinci parmak yok sayılır.
            ev(EV_ABS, ABS_MT_SLOT, 1),
            ev(EV_ABS, ABS_MT_POSITION_X, 1000),
            ev(EV_ABS, ABS_MT_SLOT, 0),
            ev(EV_ABS, ABS_MT_TRACKING_ID, -1),
            ev(EV_SYN, SYN_REPORT, 0),
        ] {
            out.extend(t.feed(e));
        }
        let (x, y1) = to_screen(100, 200, 1);
        let (_, y2) = to_screen(300, 200, 1);
        assert_eq!(
            out,
            [
                Input::Touch {
                    phase: TouchPhase::Down,
                    x,
                    y: y1
                },
                Input::Touch {
                    phase: TouchPhase::Move,
                    x,
                    y: y2
                },
                Input::Touch {
                    phase: TouchPhase::Up,
                    x,
                    y: y2
                },
            ]
        );
    }

    #[test]
    fn a_second_finger_is_reported_separately() {
        let mut t = Touch::new(Arc::new(AtomicU32::new(1)));
        let mut out = Vec::new();
        for e in [
            ev(EV_ABS, ABS_MT_TRACKING_ID, 1),
            ev(EV_ABS, ABS_MT_POSITION_X, 100),
            ev(EV_ABS, ABS_MT_POSITION_Y, 100),
            ev(EV_ABS, ABS_MT_SLOT, 1),
            ev(EV_ABS, ABS_MT_TRACKING_ID, 2),
            ev(EV_ABS, ABS_MT_POSITION_X, 500),
            ev(EV_ABS, ABS_MT_POSITION_Y, 500),
            ev(EV_SYN, SYN_REPORT, 0),
            ev(EV_ABS, ABS_MT_TRACKING_ID, -1),
            ev(EV_SYN, SYN_REPORT, 0),
        ] {
            out.extend(t.feed(e));
        }
        let (x0, y0) = to_screen(100, 100, 1);
        let (x1, y1) = to_screen(500, 500, 1);
        assert_eq!(
            out,
            [
                Input::Touch {
                    phase: TouchPhase::Down,
                    x: x0,
                    y: y0
                },
                Input::SecondTouch {
                    phase: TouchPhase::Down,
                    x: x1,
                    y: y1
                },
                Input::SecondTouch {
                    phase: TouchPhase::Up,
                    x: x1,
                    y: y1
                },
            ]
        );
    }

    #[test]
    fn single_touch_protocol_works_too() {
        let mut t = Touch::new(Arc::new(AtomicU32::new(1)));
        let phases: Vec<_> = [
            ev(EV_ABS, ABS_X, 10),
            ev(EV_ABS, ABS_Y, 20),
            ev(EV_KEY, BTN_TOUCH, 1),
            ev(EV_SYN, SYN_REPORT, 0),
            ev(EV_KEY, BTN_TOUCH, 0),
            ev(EV_SYN, SYN_REPORT, 0),
        ]
        .into_iter()
        .flat_map(|e| t.feed(e))
        .map(|i| match i {
            Input::Touch { phase, .. } => phase,
            _ => unreachable!(),
        })
        .collect();
        assert_eq!(phases, [TouchPhase::Down, TouchPhase::Up]);
    }
}
