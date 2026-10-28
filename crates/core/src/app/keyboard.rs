//! Türkçe Q ekran klavyesi: tuş yerleşimi, dokunulan tuş ve metne etkisi.
//! İmleç her zaman metnin sonundadır.

use super::ui::Rect;
use crate::LIBRA2_HEIGHT;
use crate::collate::turkish_upper;

/// Tuş yüksekliği ve klavyenin ekran kenarlarına uzaklığı (piksel).
const KEY_H: i32 = 110;
const KB_MARGIN: i32 = 16;
/// Bir satır en fazla bu kadar birim genişliğindedir (harf tuşu 1 birim).
const UNITS: f32 = 12.0;
const ROWS: i32 = 4;
/// Klavyenin üst kenarı.
pub(super) const KEYBOARD_TOP: i32 = LIBRA2_HEIGHT as i32 - ROWS * KEY_H - KB_MARGIN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Key {
    Char(char),
    Shift,
    Backspace,
    /// Harf ↔ rakam/işaret katmanı.
    Layer,
    Space,
}

const LETTERS: [&str; 3] = ["qwertyuıopğü", "asdfghjklşi", "zxcvbnmöç"];
const SYMBOLS: [&str; 3] = ["1234567890-/", ":;()\"'!?&@", "%+=*#…–"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Keyboard {
    /// Bir sonraki harf büyük yazılır (tek seferlik).
    pub shift: bool,
    pub symbols: bool,
}

impl Keyboard {
    /// Tuşlar ve ekrandaki yerleri, satır satır.
    pub fn keys(&self, width: u32) -> Vec<(Key, Rect)> {
        let unit = (width as i32 - 2 * KB_MARGIN) as f32 / UNITS;
        let rows = if self.symbols { SYMBOLS } else { LETTERS };
        let mut out = Vec::new();
        let mut row = |r: i32, keys: Vec<(Key, f32)>| {
            let total: f32 = keys.iter().map(|(_, w)| w).sum();
            let mut x = KB_MARGIN as f32 + (UNITS - total) * unit / 2.0;
            for (key, w) in keys {
                out.push((
                    key,
                    Rect {
                        x: x as i32,
                        y: KEYBOARD_TOP + r * KEY_H,
                        w: (w * unit) as i32,
                        h: KEY_H,
                    },
                ));
                x += w * unit;
            }
        };
        let chars = |s: &str| s.chars().map(|c| (Key::Char(c), 1.0)).collect::<Vec<_>>();
        row(0, chars(rows[0]));
        row(1, chars(rows[1]));
        let mut third = vec![(Key::Shift, 1.5)];
        third.extend(chars(rows[2]));
        third.push((Key::Backspace, 1.5));
        row(2, third);
        row(
            3,
            vec![
                (Key::Layer, 2.0),
                (Key::Char(','), 1.0),
                (Key::Space, 6.0),
                (Key::Char('.'), 1.0),
            ],
        );
        out
    }

    pub fn key_at(&self, width: u32, x: u32, y: u32) -> Option<Key> {
        self.keys(width)
            .into_iter()
            .find(|(_, r)| r.contains(x, y))
            .map(|(k, _)| k)
    }

    /// Tuşun metne etkisi; büyük harf tek seferliktir.
    pub fn press(&mut self, key: Key, text: &mut String) {
        match key {
            Key::Char(c) => {
                if self.shift {
                    text.push_str(&turkish_upper(c));
                    self.shift = false;
                } else {
                    text.push(c);
                }
            }
            Key::Space => text.push(' '),
            Key::Backspace => {
                text.pop();
            }
            Key::Shift => self.shift = !self.shift,
            Key::Layer => {
                self.symbols = !self.symbols;
                self.shift = false;
            }
        }
    }

    /// Harf tuşunun ekranda görünen hâli (büyük harf açıksa büyük).
    pub fn label(&self, c: char) -> String {
        if self.shift {
            turkish_upper(c)
        } else {
            c.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LIBRA2_WIDTH;

    fn type_keys(kb: &mut Keyboard, keys: &[Key]) -> String {
        let mut text = String::new();
        for k in keys {
            kb.press(*k, &mut text);
        }
        text
    }

    #[test]
    fn shift_uses_turkish_capitals_once() {
        use Key::*;
        let mut kb = Keyboard::default();
        let text = type_keys(
            &mut kb,
            &[
                Shift,
                Char('i'),
                Char('ı'),
                Space,
                Shift,
                Char('ı'),
                Char('ş'),
                Backspace,
            ],
        );
        assert_eq!(text, "İı I");
        assert!(!kb.shift);
    }

    #[test]
    fn every_key_is_on_screen_and_reachable_by_touch() {
        for kb in [
            Keyboard::default(),
            Keyboard {
                shift: false,
                symbols: true,
            },
        ] {
            let keys = kb.keys(LIBRA2_WIDTH);
            for (key, r) in &keys {
                assert!(
                    r.x >= 0 && r.x + r.w <= LIBRA2_WIDTH as i32,
                    "{key:?} {r:?}"
                );
                assert!(r.y + r.h <= LIBRA2_HEIGHT as i32, "{key:?} {r:?}");
                let (cx, cy) = ((r.x + r.w / 2) as u32, (r.y + r.h / 2) as u32);
                assert_eq!(kb.key_at(LIBRA2_WIDTH, cx, cy), Some(*key));
            }
            // Türkçe harflerin hepsi klavyede.
            if !kb.symbols {
                for c in "çğıöşü".chars() {
                    assert!(keys.iter().any(|(k, _)| *k == Key::Char(c)), "{c}");
                }
            }
        }
    }
}
