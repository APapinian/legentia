//! Metin seçimi ve vurgu çizimi (dizilmiş metin sayfaları: EPUB ve yeniden
//! akıtılmış PDF). Uzun basma kelimeyi seçer, sürükleme seçimi kelime kelime uzatır.

use super::session::{Page, PlacedLine, Position};
use super::ui::{MARGIN, Rect, TOP, draw_button};
use super::{App, View};
use crate::LIBRA2_HEIGHT;
use crate::canvas::Canvas;
use crate::epub::Block;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};

/// İşlem çubuğu düğmelerinin yüksekliği ve aralığı (piksel).
const ACTION_H: i32 = 100;
const ACTION_GAP: i32 = 20;

/// Vurgunun zemini; siyah yazı üstünde okunur kalır.
pub(super) const HIGHLIGHT_GRAY: u8 = 215;
/// Seçimin zemini; vurgudan koyu, ayırt edilsin.
pub(super) const SELECTION_GRAY: u8 = 170;

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

/// Satırın `[start, end)` aralığına düşen kısmının yatay sınırı (satırın sol
/// kenarına göre piksel).
fn line_span(line: &PlacedLine, start: Position, end: Position) -> Option<(f32, f32)> {
    let inside = |cluster: usize| {
        let p = Position {
            block: line.block,
            offset: cluster,
        };
        p >= start && p < end
    };
    line.line
        .glyphs
        .iter()
        .filter(|(g, _)| inside(g.cluster))
        .fold(None, |acc, (g, gx)| {
            let (a, b) = acc.unwrap_or((*gx, gx + g.advance));
            Some((a.min(*gx), b.max(gx + g.advance)))
        })
}

/// Sayfadaki her satırda `[start, end)` aralığının zeminini boyar (yazıdan önce çizilmeli).
pub(super) fn paint(
    canvas: &mut Canvas,
    page: &Page,
    margin: i32,
    start: Position,
    end: Position,
    gray: u8,
) {
    for line in &page.lines {
        if let Some((a, b)) = line_span(line, start, end) {
            canvas.fill_rect(
                margin + a as i32,
                line.top,
                (b - a).ceil() as u32,
                line.height as u32,
                gray,
            );
        }
    }
}

/// Ekrandaki noktaya düşen vurgu: `placed` içinden (sıra, başlangıç, bitiş).
pub(super) fn annotation_at(
    page: &Page,
    blocks: &[Block],
    placed: &[(usize, Position, Position)],
    margin: i32,
    x: u32,
    y: u32,
) -> Option<usize> {
    let (s, _) = word_at(page, blocks, margin, x, y)?;
    placed
        .iter()
        .find(|(_, a, b)| s >= *a && s < *b)
        .map(|(i, _, _)| *i)
}

/// İşlem çubuğundaki düğmeler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Highlight,
    Remove,
    Cancel,
}

impl App {
    /// Okunan sayfa dizilmiş metinse (sayfa görüntüsü değil) sayfa ve bloklar.
    fn text_page(&self) -> Option<(&Page, &[Block])> {
        let s = self.session.as_ref().filter(|s| !s.showing_pdf_image())?;
        Some((s.pages.get(s.page)?, &s.blocks))
    }

    fn word_at(&self, x: u32, y: u32) -> Option<(Position, Position)> {
        let (page, blocks) = self.text_page()?;
        word_at(page, blocks, self.settings.margin, x, y)
    }

    pub(super) fn start_selection(&mut self, x: u32, y: u32) -> Option<Refresh> {
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

    pub(super) fn annotation_at(&self, x: u32, y: u32) -> Option<usize> {
        let (page, blocks) = self.text_page()?;
        let s = self.session.as_ref()?;
        annotation_at(page, blocks, &s.placed, self.settings.margin, x, y)
    }

    fn actions(&self) -> &'static [Action] {
        match self.view {
            View::Selected => &[Action::Highlight, Action::Cancel],
            View::Annotation { .. } => &[Action::Remove, Action::Cancel],
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

    /// Seçimin ya da dokunulan vurgunun sayfadaki ilk satırının üstü.
    fn marked_top(&self) -> Option<i32> {
        let (page, _) = self.text_page()?;
        let s = self.session.as_ref()?;
        let (a, b) = match self.view {
            View::Annotation { index } => s
                .placed
                .iter()
                .find(|(i, _, _)| *i == index)
                .map(|(_, a, b)| (*a, *b))?,
            _ => self.selection.map(|sel| (sel.start, sel.end))?,
        };
        page.lines
            .iter()
            .find(|l| line_span(l, a, b).is_some())
            .map(|l| l.top)
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
        if self.tapped_action(event) == Some(Action::Highlight)
            && let (Some(sel), Some(s)) = (self.selection, self.session.as_mut())
        {
            s.add_annotation(sel.start, sel.end);
            self.save_progress();
        }
        // Başka her dokunma ya da tuş seçimi bırakır.
        self.selection = None;
        self.view = View::Reading;
        Some(self.refresh(false))
    }

    pub(super) fn handle_annotation(&mut self, index: usize, event: Event) -> Option<Refresh> {
        if self.tapped_action(event) == Some(Action::Remove)
            && let Some(s) = self.session.as_mut()
        {
            s.remove_annotation(index);
            self.save_progress();
        }
        self.view = View::Reading;
        Some(self.refresh(false))
    }

    pub(super) fn draw_actions(&self, canvas: &mut Canvas) {
        for (i, action) in self.actions().iter().enumerate() {
            let label = match action {
                Action::Highlight => Msg::Highlight,
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
