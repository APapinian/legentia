//! Metin seçimi ve vurgu çizimi. Uzun basma kelimeyi seçer, sürükleme seçimi
//! kelime kelime uzatır. Dizilmiş metinde (EPUB, yeniden akıtılmış PDF) satırların
//! glifleri, PDF sayfa görüntüsünde sayfa metninin karakter kutuları kullanılır.
//! Vurgu, sayfa çizildikten sonra dikdörtgenleri koyulaştırarak çizilir; yazı siyah kalır.

use super::pdf_view;
use super::session::{Page, PlacedLine, Position};
use super::ui::{MARGIN, Rect, TOP, draw_button};
use super::{App, View};
use crate::LIBRA2_HEIGHT;
use crate::canvas::Canvas;
use crate::epub::Block;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};
use crate::pdf::{Area, PageText};

/// İşlem çubuğu düğmelerinin yüksekliği ve aralığı (piksel).
const ACTION_H: i32 = 100;
const ACTION_GAP: i32 = 20;

/// Vurgunun zemini; siyah yazı üstünde okunur kalır.
pub(super) const HIGHLIGHT_GRAY: u8 = 215;
/// Seçimin zemini; vurgudan koyu, ayırt edilsin.
pub(super) const SELECTION_GRAY: u8 = 170;
/// Notlu vurgunun sonundaki işaretin kenarı (piksel).
const NOTE_MARK: u32 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Selection {
    /// Uzun basılan kelime; sürükleme yönünden bağımsız seçimde kalır.
    anchor: (Position, Position),
    pub start: Position,
    pub end: Position,
}

impl Selection {
    pub fn new(word: (Position, Position)) -> Self {
        Self {
            anchor: word,
            start: word.0,
            end: word.1,
        }
    }

    /// Seçimi bağlantı kelimesinden `word`'e kadar uzatır (geriye de olabilir).
    pub fn extend(&mut self, word: (Position, Position)) {
        (self.start, self.end) = if word.0 < self.anchor.0 {
            (word.0, self.anchor.1)
        } else {
            (self.anchor.0, word.1.max(self.anchor.1))
        };
    }
}

/// Ekrandaki noktanın altındaki kelime. Satırların arasına ya da sayfa dışına
/// düşen nokta `None`; satırın solu/sağı satırın ilk/son kelimesidir.
pub(super) fn word_at(
    page: &Page,
    blocks: &[Block],
    margin: i32,
    x: u32,
    y: u32,
) -> Option<(Position, Position)> {
    let y = y as i32;
    let line = page
        .lines
        .iter()
        .find(|l| y >= l.top && y < l.top + l.height)?;
    let rel = x as f32 - margin as f32;
    let glyph = line
        .line
        .glyphs
        .iter()
        .find(|(g, gx)| rel < gx + g.advance)
        .or(line.line.glyphs.last())?;
    let text = &blocks.get(line.block)?.text;
    let (s, e) = word_bounds(text, glyph.0.cluster.min(text.len()));
    let at = |offset| Position {
        block: line.block,
        offset,
    };
    Some((at(s), at(e)))
}

/// `offset`'teki karakterin kelimesi (harf ve rakamlar; harfler arasındaki
/// kesme işareti kelimeye dahil: "İstanbul'dan"). Kelime karakteri değilse yalnızca o karakter.
fn word_bounds(text: &str, offset: usize) -> (usize, usize) {
    let is_word = |i: usize, c: char| {
        c.is_alphanumeric()
            || (matches!(c, '\'' | '’')
                && text[..i]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphanumeric)
                && text[i + c.len_utf8()..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphanumeric))
    };
    let Some(c) = text[offset..].chars().next() else {
        return (offset, offset);
    };
    if !is_word(offset, c) {
        return (offset, offset + c.len_utf8());
    }
    let mut start = offset;
    while let Some((i, c)) = text[..start].char_indices().next_back()
        && is_word(i, c)
    {
        start = i;
    }
    let mut end = offset;
    while let Some(c) = text[end..].chars().next()
        && is_word(end, c)
    {
        end += c.len_utf8();
    }
    (start, end)
}

/// Satırın `[start, end)` aralığına düşen kısmının ekrandaki dikdörtgeni.
fn line_rect(line: &PlacedLine, margin: i32, start: Position, end: Position) -> Option<Rect> {
    let inside = |cluster: usize| {
        let p = Position {
            block: line.block,
            offset: cluster,
        };
        p >= start && p < end
    };
    let (a, b) = line
        .line
        .glyphs
        .iter()
        .filter(|(g, _)| inside(g.cluster))
        .fold(None, |acc: Option<(f32, f32)>, (g, gx)| {
            let (a, b) = acc.unwrap_or((*gx, gx + g.advance));
            Some((a.min(*gx), b.max(gx + g.advance)))
        })?;
    Some(Rect {
        x: margin + a as i32,
        y: line.top,
        w: (b - a).ceil() as i32,
        h: line.height,
    })
}

/// Sayfa noktasının (punto) altındaki karakter: (blok, bayt). Kutuya düşmeyen
/// nokta, dikeyde içine düştüğü satırın en yakın karakterine bağlanır (en fazla
/// bir yazı boyu uzaktaysa); kelimeler arası boşluğa basmak da kelime seçer.
fn char_at(text: &PageText, x: f32, y: f32) -> Option<(usize, usize)> {
    text.boxes
        .iter()
        .enumerate()
        .flat_map(|(block, boxes)| boxes.iter().map(move |b| (block, b)))
        .filter(|(_, b)| y >= b.area.y && y < b.area.y + b.area.h)
        .map(|(block, b)| {
            let dx = (b.area.x - x).max(x - (b.area.x + b.area.w)).max(0.0);
            (dx, b.area.h, block, b.start)
        })
        .filter(|(dx, h, ..)| *dx <= *h)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, _, block, offset)| (block, offset))
}

/// `[start, end)` aralığındaki karakter kutuları, aynı satırdakiler birleştirilerek (punto).
pub(super) fn box_areas(text: &PageText, start: Position, end: Position) -> Vec<Area> {
    let mut out: Vec<Area> = Vec::new();
    for (block, boxes) in text.boxes.iter().enumerate() {
        for b in boxes {
            let p = Position {
                block,
                offset: b.start,
            };
            if p < start || p >= end {
                continue;
            }
            let a = b.area;
            match out.last_mut() {
                // Aynı satırda, sağa doğru devam ediyor.
                Some(last) if (a.y - last.y).abs() < 0.5 * a.h && a.x >= last.x => {
                    let right = (last.x + last.w).max(a.x + a.w);
                    let bottom = (last.y + last.h).max(a.y + a.h);
                    last.y = last.y.min(a.y);
                    last.w = right - last.x;
                    last.h = bottom - last.y;
                }
                _ => out.push(a),
            }
        }
    }
    out
}

/// İşlem çubuğundaki düğmeler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Highlight,
    /// Not ekranını açar; seçimdeyse önce vurgular.
    Note,
    Remove,
    Cancel,
}

impl App {
    /// Ekrandaki noktanın altındaki kelime (okunan bölümün metin bloklarında).
    fn word_at(&self, x: u32, y: u32) -> Option<(Position, Position)> {
        let s = self.session.as_ref()?;
        let page = s.pages.get(s.page)?;
        if !s.showing_pdf_image() {
            return word_at(page, &s.blocks, self.settings.margin, x, y);
        }
        let (text, screen) = (s.pdf_text.as_ref()?, page.screen?);
        let (px, py) = pdf_view::to_page(&screen, self.width, x, y);
        let (block, offset) = char_at(text, px, py)?;
        let (a, b) = word_bounds(&text.blocks[block].text, offset);
        Some((Position { block, offset: a }, Position { block, offset: b }))
    }

    /// `[start, end)` aralığının okunan ekrandaki dikdörtgenleri.
    fn range_rects(&self, start: Position, end: Position) -> Vec<Rect> {
        let Some(s) = self.session.as_ref() else {
            return Vec::new();
        };
        let Some(page) = s.pages.get(s.page) else {
            return Vec::new();
        };
        if !s.showing_pdf_image() {
            return page
                .lines
                .iter()
                .filter_map(|l| line_rect(l, self.settings.margin, start, end))
                .collect();
        }
        match (s.pdf_text.as_ref(), page.screen) {
            (Some(text), Some(screen)) => box_areas(text, start, end)
                .into_iter()
                .map(|a| pdf_view::to_screen(&screen, self.width, a))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Okunan ekrandaki vurguları ve seçimi çizer (sayfa çizildikten sonra).
    pub(super) fn paint_marks(&self, canvas: &mut Canvas) {
        let Some(s) = self.session.as_ref() else {
            return;
        };
        let noted = |i: usize| s.state.annotations.get(i).is_some_and(|a| a.note.is_some());
        let marks = s
            .placed
            .iter()
            .map(|(i, a, b)| (*a, *b, HIGHLIGHT_GRAY, noted(*i)))
            .chain(
                self.selection
                    .map(|sel| (sel.start, sel.end, SELECTION_GRAY, false)),
            );
        for (a, b, gray, note) in marks {
            let rects = self.range_rects(a, b);
            for r in &rects {
                canvas.darken_rect(r.x, r.y, r.w.max(0) as u32, r.h.max(0) as u32, gray);
            }
            // Notlu vurgunun sonunda küçük siyah işaret.
            if note && let Some(r) = rects.last() {
                canvas.fill_rect(r.x + r.w, r.y, NOTE_MARK, NOTE_MARK, 0);
            }
        }
    }

    pub(super) fn start_selection(&mut self, x: u32, y: u32) -> Option<Refresh> {
        // Sayfa görüntüsünde metin yalnızca gerektiğinde çıkarılır.
        self.session.as_mut()?.load_page_text();
        let word = self.word_at(x, y)?;
        self.selection = Some(Selection::new(word));
        self.view = View::Reading;
        Some(self.refresh(false))
    }

    pub(super) fn extend_selection(&mut self, x: u32, y: u32) -> Option<Refresh> {
        let word = self.word_at(x, y)?;
        let sel = self.selection.as_mut()?;
        let before = *sel;
        sel.extend(word);
        (*sel != before).then(|| self.refresh(false))
    }

    pub(super) fn finish_selection(&mut self) -> Option<Refresh> {
        self.selection?;
        self.view = View::Selected;
        Some(self.refresh(false))
    }

    /// Dokunulan vurgunun `state.annotations` sırası.
    pub(super) fn annotation_at(&self, x: u32, y: u32) -> Option<usize> {
        let s = self.session.as_ref()?;
        if s.placed.is_empty() {
            return None;
        }
        let (at, _) = self.word_at(x, y)?;
        s.placed
            .iter()
            .find(|(_, a, b)| at >= *a && at < *b)
            .map(|(i, _, _)| *i)
    }

    fn actions(&self) -> &'static [Action] {
        match self.view {
            View::Selected => &[Action::Highlight, Action::Note, Action::Cancel],
            View::Annotation { .. } => &[Action::Note, Action::Remove, Action::Cancel],
            _ => &[],
        }
    }

    /// İşlem çubuğunun `i`. düğmesi. Çubuk, seçim ya da vurgu ekranın alt
    /// yarısındaysa üstte, değilse altta durur; metni örtmesin.
    pub(super) fn action_rect(&self, i: usize) -> Rect {
        let n = self.actions().len().max(1) as i32;
        let w = (self.width as i32 - 2 * MARGIN - (n - 1) * ACTION_GAP) / n;
        let y = if self
            .marked_top()
            .is_some_and(|t| t > LIBRA2_HEIGHT as i32 / 2)
        {
            TOP - ACTION_H / 2
        } else {
            LIBRA2_HEIGHT as i32 - 2 * ACTION_H
        };
        Rect {
            x: MARGIN + i as i32 * (w + ACTION_GAP),
            y,
            w,
            h: ACTION_H,
        }
    }

    /// Seçimin ya da dokunulan vurgunun ekrandaki üst kenarı.
    fn marked_top(&self) -> Option<i32> {
        let s = self.session.as_ref()?;
        let (a, b) = match self.view {
            View::Annotation { index } => s
                .placed
                .iter()
                .find(|(i, _, _)| *i == index)
                .map(|(_, a, b)| (*a, *b))?,
            _ => self.selection.map(|sel| (sel.start, sel.end))?,
        };
        self.range_rects(a, b).iter().map(|r| r.y).min()
    }

    fn tapped_action(&self, event: Event) -> Option<Action> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        let actions = self.actions();
        (0..actions.len())
            .find(|&i| self.action_rect(i).contains(x, y))
            .map(|i| actions[i])
    }

    pub(super) fn handle_selected(&mut self, event: Event) -> Option<Refresh> {
        if let Event::Hold { x, y } = event {
            return self.start_selection(x, y);
        }
        let action = self.tapped_action(event);
        let added = match (action, self.selection, self.session.as_mut()) {
            (Some(Action::Highlight | Action::Note), Some(sel), Some(s)) => {
                s.add_annotation(sel.start, sel.end)
            }
            _ => None,
        };
        // Başka her dokunma ya da tuş seçimi bırakır.
        self.selection = None;
        if let Some(index) = added {
            self.save_progress();
            if action == Some(Action::Note) {
                return self.open_note(index);
            }
        }
        self.view = View::Reading;
        Some(self.refresh(false))
    }

    pub(super) fn handle_annotation(&mut self, index: usize, event: Event) -> Option<Refresh> {
        match self.tapped_action(event) {
            Some(Action::Note) => return self.open_note(index),
            Some(Action::Remove) => {
                if let Some(s) = self.session.as_mut() {
                    s.remove_annotation(index);
                }
                self.save_progress();
            }
            _ => {}
        }
        self.view = View::Reading;
        Some(self.refresh(false))
    }

    /// Dokunulan vurgunun notu.
    fn opened_note(&self) -> Option<&str> {
        let View::Annotation { index } = self.view else {
            return None;
        };
        self.session
            .as_ref()?
            .state
            .annotations
            .get(index)?
            .note
            .as_deref()
    }

    pub(super) fn draw_actions(&self, canvas: &mut Canvas) {
        let note = self.opened_note();
        if let Some(note) = note {
            self.draw_note_panel(canvas, note);
        }
        for (i, action) in self.actions().iter().enumerate() {
            let label = match action {
                Action::Highlight => Msg::Highlight,
                Action::Note if note.is_some() => Msg::EditNote,
                Action::Note => Msg::AddNote,
                Action::Remove => Msg::RemoveHighlight,
                Action::Cancel => Msg::Cancel,
            };
            draw_button(&self.text, canvas, self.action_rect(i), &tr(&label), 40.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, offset: usize) -> &str {
        let (s, e) = word_bounds(text, offset);
        &text[s..e]
    }

    #[test]
    fn words_include_turkish_letters_and_inner_apostrophes() {
        let t = "Kardeşimiz İstanbul'dan geldi, 'evet' dedi.";
        assert_eq!(word(t, 3), "Kardeşimiz");
        assert_eq!(word(t, t.find("tan").unwrap()), "İstanbul'dan");
        assert_eq!(
            word(t, t.find("evet").unwrap()),
            "evet",
            "tırnak kelimeye girmez"
        );
        assert_eq!(word(t, t.find(',').unwrap()), ",");
        assert_eq!(word(t, t.len()), "");
    }

    #[test]
    fn selection_extends_both_ways_from_the_held_word() {
        let p = |o| Position {
            block: 0,
            offset: o,
        };
        let mut s = Selection::new((p(10), p(15)));
        s.extend((p(20), p(25)));
        assert_eq!((s.start, s.end), (p(10), p(25)));
        s.extend((p(0), p(4)));
        assert_eq!((s.start, s.end), (p(0), p(15)));
        s.extend((p(11), p(13)));
        assert_eq!(
            (s.start, s.end),
            (p(10), p(15)),
            "kelimenin içi kelimeyi bırakmaz"
        );
    }
}
