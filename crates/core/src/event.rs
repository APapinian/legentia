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
    /// Parmak kayarak kalktı (uzun basma olmadan); başlangıçtan bitişe yer değiştirme.
    Swipe {
        dx: i32,
        dy: i32,
    },
    /// İki parmak açıldı ya da kapandı: son aralık / ilk aralık × 1000.
    Pinch {
        permille: u32,
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
    /// Cihazın tutuluşu değişti (ivmeölçer): yatay mı.
    Tilt {
        landscape: bool,
    },
    /// İkinci parmağın ham dokunması (iki parmak hareketi için).
    SecondTouch {
        phase: TouchPhase,
        x: u32,
        y: u32,
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
/// önce kayan dokunma kalkınca kaydırmadır. İkinci parmak değince iki parmak
/// hareketi başlar; parmaklardan biri kalkınca biter, ilk parmak artık
/// dokunma ya da uzun basma üretmez.
#[derive(Debug, Default)]
pub(crate) struct Gestures {
    touch: Option<Touch>,
    /// İkinci parmağın son konumu.
    second: Option<(u32, u32)>,
    /// İki parmak hareketinin başındaki aralık.
    pinch_from: Option<f32>,
}

#[derive(Debug)]
struct Touch {
    x: u32,
    y: u32,
    ms: u64,
    holding: bool,
    slid: bool,
    /// Son bilinen konum.
    at: (u32, u32),
    /// İki parmak hareketinin parçası: dokunma/uzun basma/kaydırma üretmez.
    pinching: bool,
}

fn distance(a: (u32, u32), b: (u32, u32)) -> f32 {
    let (dx, dy) = (a.0 as f32 - b.0 as f32, a.1 as f32 - b.1 as f32);
    (dx * dx + dy * dy).sqrt()
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
                    at: (x, y),
                    pinching: false,
                });
            }
            return Vec::new();
        };
        t.at = (x, y);
        if t.pinching {
            if phase == TouchPhase::Up {
                let pinch = self.end_pinch();
                self.touch = None;
                return pinch.into_iter().collect();
            }
            return Vec::new();
        }
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
                    at: (x, y),
                    pinching: false,
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
                } else {
                    out.push(Event::Swipe {
                        dx: x as i32 - t.x as i32,
                        dy: y as i32 - t.y as i32,
                    });
                }
                self.touch = None;
            }
        }
        out
    }

    /// İkinci parmak. İlk parmak dokunurken değerse iki parmak hareketi başlar.
    pub fn feed_second(&mut self, phase: TouchPhase, x: u32, y: u32) -> Vec<Event> {
        match phase {
            TouchPhase::Down => {
                self.second = Some((x, y));
                if let Some(t) = self.touch.as_mut()
                    && !t.holding
                {
                    t.pinching = true;
                    self.pinch_from = Some(distance(t.at, (x, y)));
                }
                Vec::new()
            }
            TouchPhase::Move => {
                self.second = Some((x, y));
                Vec::new()
            }
            TouchPhase::Up => {
                self.second = Some((x, y));
                let pinch = self.end_pinch();
                self.second = None;
                pinch.into_iter().collect()
            }
        }
    }

    fn end_pinch(&mut self) -> Option<Event> {
        let from = self.pinch_from.take()?;
        let to = distance(self.touch.as_ref()?.at, self.second?);
        (from > 0.0).then(|| Event::Pinch {
            permille: (to / from * 1000.0).round() as u32,
        })
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
    fn a_quick_slide_is_a_swipe() {
        assert_eq!(
            run(&[
                (Down, 600, 100, 0),
                (Move, 400, 100, 100),
                (Up, 200, 120, 900)
            ]),
            vec![Event::Swipe { dx: -400, dy: 20 }]
        );
    }

    #[test]
    fn two_fingers_spreading_make_a_pinch() {
        let mut g = Gestures::default();
        let mut out = g.feed(Down, 500, 800, 0);
        out.extend(g.feed_second(Down, 700, 800));
        out.extend(g.feed(Move, 400, 800, 100));
        out.extend(g.feed_second(Move, 800, 800));
        out.extend(g.feed_second(Up, 800, 800));
        out.extend(g.feed(Up, 400, 800, 900));
        // 200 px aralık 400 px oldu; ilk parmağın kalkması dokunma sayılmaz.
        assert_eq!(out, vec![Event::Pinch { permille: 2000 }]);
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
