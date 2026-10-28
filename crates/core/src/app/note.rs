//! Not ekranı: vurgunun alıntısı, yazılan not ve ekran klavyesi.

use super::keyboard::{KEYBOARD_TOP, Key, Keyboard};
use super::ui::{HEADER_BOTTOM, MARGIN, PLAIN, ROW_PX, Rect, draw_button, draw_header};
use super::{App, View};
use crate::canvas::Canvas;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};
use crate::text::Family;

/// Not metninin yazı boyutu ve satır yüksekliği (piksel).
const NOTE_PX: f32 = 40.0;
const NOTE_LINE: i32 = 56;
/// Kaydet/Vazgeç düğmelerinin yüksekliği ve klavyeye uzaklığı.
const BUTTON_H: i32 = 100;
const BUTTON_GAP: i32 = 30;
/// Okurken gösterilen not kutusunun en fazla satır sayısı.
const PANEL_LINES: usize = 6;
/// Not alanının altı; altında düğmeler ve klavye.
const NOTE_BOTTOM: i32 = KEYBOARD_TOP - BUTTON_H - 2 * BUTTON_GAP;

impl App {
    /// `index`. vurgunun notunu düzenlemeye başlar.
    pub(super) fn open_note(&mut self, index: usize) -> Option<Refresh> {
        let draft = self
            .session
            .as_ref()?
            .state
            .annotations
            .get(index)?
            .note
            .clone()
            .unwrap_or_default();
        self.view = View::Note {
            index,
            draft,
            keyboard: Keyboard::default(),
        };
        Some(self.refresh(true))
    }

    /// (Vazgeç, Kaydet) düğmeleri.
    pub(super) fn note_buttons(&self) -> (Rect, Rect) {
        let w = (self.width as i32 - 2 * MARGIN - BUTTON_GAP) / 2;
        let y = KEYBOARD_TOP - BUTTON_H - BUTTON_GAP;
        let r = |i: i32| Rect {
            x: MARGIN + i * (w + BUTTON_GAP),
            y,
            w,
            h: BUTTON_H,
        };
        (r(0), r(1))
    }

    pub(super) fn handle_note(
        &mut self,
        index: usize,
        mut draft: String,
        mut keyboard: Keyboard,
        event: Event,
    ) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        let (cancel, save) = self.note_buttons();
        if save.contains(x, y) {
            if let Some(s) = self.session.as_mut() {
                let note = draft.trim();
                s.set_note(index, (!note.is_empty()).then(|| note.to_string()));
            }
            self.save_progress();
            self.view = View::Reading;
            return Some(self.refresh(true));
        }
        if cancel.contains(x, y) {
            self.view = View::Reading;
            return Some(self.refresh(true));
        }
        let key = keyboard.key_at(self.width, x, y)?;
        keyboard.press(key, &mut draft);
        self.view = View::Note {
            index,
            draft,
            keyboard,
        };
        Some(self.refresh(false))
    }

    pub(super) fn draw_note(&self, canvas: &mut Canvas, index: usize, draft: &str, kb: &Keyboard) {
        let t = &self.text;
        let quote = self
            .session
            .as_ref()
            .and_then(|s| s.state.annotations.get(index))
            .map_or(String::new(), |a| {
                format!("“{}”", a.quote.replace('\n', " "))
            });
        draw_header(t, canvas, &tr(&Msg::Note), &quote, false);

        // Yazılan metnin sığan son satırları; sonunda imleç.
        let max_w = (self.width as i32 - 2 * MARGIN) as f32;
        let mut lines = t.wrap_plain(draft, Family::Ui, PLAIN, NOTE_PX, max_w);
        if lines.is_empty() || draft.ends_with(' ') && lines.last().is_some_and(|l| !l.is_empty()) {
            // Sondaki boşluk satırda görünmez; imleç yine de bir boşluk ileride durur.
            lines.push(String::new());
        }
        let fits = ((NOTE_BOTTOM - HEADER_BOTTOM - 20) / NOTE_LINE).max(1) as usize;
        let shown = &lines[lines.len().saturating_sub(fits)..];
        let mut baseline = HEADER_BOTTOM + NOTE_LINE;
        for line in shown {
            t.draw_plain(canvas, line, Family::Ui, PLAIN, MARGIN, baseline, NOTE_PX);
            baseline += NOTE_LINE;
        }
        let last = shown.last().map_or("", String::as_str);
        let cursor_x = MARGIN + t.width_plain(last, Family::Ui, PLAIN, NOTE_PX) as i32 + 4;
        canvas.fill_rect(
            cursor_x,
            baseline - NOTE_LINE - NOTE_PX as i32,
            3,
            (NOTE_PX * 1.2) as u32,
            0,
        );

        let (cancel, save) = self.note_buttons();
        draw_button(t, canvas, cancel, &tr(&Msg::Cancel), ROW_PX);
        draw_button(t, canvas, save, &tr(&Msg::Save), ROW_PX);
        for (key, r) in kb.keys(self.width) {
            let label = match key {
                Key::Char(c) => kb.label(c),
                Key::Shift => tr(&Msg::KeyShift),
                Key::Backspace => tr(&Msg::KeyDelete),
                Key::Space => tr(&Msg::KeySpace),
                Key::Layer if kb.symbols => tr(&Msg::KeyLetters),
                Key::Layer => tr(&Msg::KeySymbols),
            };
            draw_button(t, canvas, r, &label, ROW_PX);
            if key == Key::Shift && kb.shift {
                // Büyük harf açık: tuş koyu çerçeveli.
                canvas.stroke_rect(r.x + 6, r.y + 6, (r.w - 12) as u32, (r.h - 12) as u32, 4);
            }
        }
    }

    /// Dokunulan vurgunun notu, işlem çubuğunun yanında çerçeveli kutuda (en çok
    /// `PANEL_LINES` satır).
    pub(super) fn draw_note_panel(&self, canvas: &mut Canvas, note: &str) {
        let t = &self.text;
        let max_w = (self.width as i32 - 4 * MARGIN) as f32;
        let mut lines = t.wrap_plain(note, Family::Ui, PLAIN, NOTE_PX, max_w);
        if lines.len() > PANEL_LINES {
            lines.truncate(PANEL_LINES);
            if let Some(last) = lines.last_mut() {
                last.push('…');
            }
        }
        let h = lines.len() as i32 * NOTE_LINE + 40;
        let bar = self.action_rect(0);
        let y = if bar.y < crate::LIBRA2_HEIGHT as i32 / 2 {
            bar.y + bar.h + 20
        } else {
            bar.y - 20 - h
        };
        let w = self.width as i32 - 2 * MARGIN;
        canvas.fill_rect(MARGIN, y, w as u32, h as u32, 255);
        canvas.stroke_rect(MARGIN, y, w as u32, h as u32, 3);
        for (i, line) in lines.iter().enumerate() {
            let baseline = y + 20 + (i as i32 + 1) * NOTE_LINE - 14;
            t.draw_plain(
                canvas,
                line,
                Family::Ui,
                PLAIN,
                2 * MARGIN,
                baseline,
                NOTE_PX,
            );
        }
    }
}
