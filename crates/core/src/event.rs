/// Platformdan gelen girdi olayları. Koordinatlar ekran pikselidir.
///
/// Platform dokunmatik ekranı ham `Touch` olarak verir; [`crate::App`] bunu
/// [`Gestures`] ile `Tap`/`Hold`/`HoldMove`/`HoldEnd` olaylarına çevirir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Tap {
        x: u32,
        y: u32,
    },
    /// Uzun basma: parmak kıpırdamadan `HOLD_MS` kadar durdu.
    Hold {
        x: u32,
        y: u32,
    },
    /// Uzun basmadan sonra parmak sürükleniyor.
    HoldMove {
        x: u32,
        y: u32,
    },
    /// Uzun basmadan sonra parmak kalktı.
    HoldEnd {
        x: u32,
        y: u32,
    },
    Key(PageKey),
    /// Arka plan işi sürerken platformun düzenli gönderdiği vuruş
    /// ([`crate::App::wants_ticks`] doğruyken); iş bitince ekran güncellenir.
    Tick,
    /// Ham dokunma; `ms` platformun tekdüze saatidir (milisaniye).
    Touch {
        phase: TouchPhase,
        x: u32,
        y: u32,
        ms: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    Down,
    Move,
    Up,
}

/// Libra 2'deki fiziksel sayfa tuşları (üst: geri, alt: ileri).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKey {
    Back,
    Forward,
}

/// E Ink yenileme türü. Kısmi yenileme hızlıdır ama gölge (ghosting) bırakır;
/// tam yenileme gölgeyi temizler ama ekranı bir kez yanıp söndürür.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    Partial,
    Full,
}

/// Bu süre kıpırdamadan basılı tutulan dokunma uzun basmadır (milisaniye).
const HOLD_MS: u64 = 500;
/// Parmağın titremesi sayılan en büyük kayma (piksel; 300 ppi'de ~2,5 mm).
const SLOP: u32 = 30;

/// Ham dokunmalardan hareket tanıyıcı. Zamanlayıcı yoktur: uzun basma, süre
/// dolduktan sonraki ilk hareket ya da kaldırma olayında tanınır. Uzun basmadan
/// önce kayan dokunma (kaydırma) şimdilik yok sayılır.
#[derive(Debug, Default)]
pub(crate) struct Gestures {
    touch: Option<Touch>,
}

#[derive(Debug)]
struct Touch {
    x: u32,
    y: u32,
    ms: u64,
    holding: bool,
    slid: bool,
}

impl Gestures {
    pub fn feed(&mut self, phase: TouchPhase, x: u32, y: u32, ms: u64) -> Vec<Event> {
        let Some(t) = self.touch.as_mut() else {
            if phase == TouchPhase::Down {
                self.touch = Some(Touch {
                    x,
                    y,
                    ms,
                    holding: false,
                    slid: false,
                });
            }
            return Vec::new();
        };
        let held = ms.saturating_sub(t.ms) >= HOLD_MS;
        let far = x.abs_diff(t.x) > SLOP || y.abs_diff(t.y) > SLOP;
        let mut out = Vec::new();
        // Süre dolduysa ve parmak henüz kaymadıysa uzun basma başlangıç noktasında başlar.
        if !t.holding && !t.slid && held {
            t.holding = true;
            out.push(Event::Hold { x: t.x, y: t.y });
        }
        match phase {
            TouchPhase::Down => {
                // Kaçırılmış kaldırma: yeni dokunma baştan başlar.
                *t = Touch {
                    x,
                    y,
                    ms,
                    holding: false,
                    slid: false,
                };
                out.clear();
            }
            TouchPhase::Move if t.holding => out.push(Event::HoldMove { x, y }),
            TouchPhase::Move => t.slid |= far,
            TouchPhase::Up => {
                if t.holding {
                    out.push(Event::HoldEnd { x, y });
                } else if !t.slid && !far {
                    out.push(Event::Tap { x: t.x, y: t.y });
                }
                self.touch = None;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::TouchPhase::*;
    use super::*;

    fn run(steps: &[(TouchPhase, u32, u32, u64)]) -> Vec<Event> {
        let mut g = Gestures::default();
        steps
            .iter()
            .flat_map(|&(p, x, y, ms)| g.feed(p, x, y, ms))
            .collect()
    }

    #[test]
    fn short_touch_is_a_tap_even_with_jitter() {
        assert_eq!(
            run(&[(Down, 100, 100, 0), (Move, 110, 95, 50), (Up, 112, 96, 120)]),
            vec![Event::Tap { x: 100, y: 100 }]
        );
    }

    #[test]
    fn hold_then_drag_selects() {
        assert_eq!(
            run(&[
                (Down, 100, 100, 0),
                (Move, 105, 100, 300),
                (Move, 300, 140, 700),
                (Up, 400, 150, 900),
            ]),
            vec![
                Event::Hold { x: 100, y: 100 },
                Event::HoldMove { x: 300, y: 140 },
                Event::HoldEnd { x: 400, y: 150 },
            ]
        );
    }

    #[test]
    fn hold_without_moving_is_recognised_on_release() {
        assert_eq!(
            run(&[(Down, 100, 100, 0), (Up, 101, 100, 800)]),
            vec![
                Event::Hold { x: 100, y: 100 },
                Event::HoldEnd { x: 101, y: 100 }
            ]
        );
    }

    #[test]
    fn a_quick_slide_is_neither_tap_nor_hold() {
        assert!(
            run(&[
                (Down, 100, 100, 0),
                (Move, 400, 100, 100),
                (Up, 600, 100, 900)
            ])
            .is_empty()
        );
    }

    #[test]
    fn a_missed_release_does_not_stick() {
        assert_eq!(
            run(&[
                (Down, 100, 100, 0),
                (Down, 500, 500, 2000),
                (Up, 500, 500, 2100)
            ]),
            vec![Event::Tap { x: 500, y: 500 }]
        );
    }
}
