//! Ekranların ortak parçaları: ölçüler, listeler, düğmeler, başlık ve alt bilgi.

use crate::LIBRA2_HEIGHT;
use crate::canvas::Canvas;
use crate::i18n::{Msg, tr};
use crate::text::{Family, Style, Text};

pub(super) const TOP: i32 = 90;
pub(super) const BOTTOM: i32 = LIBRA2_HEIGHT as i32 - 110;
pub(super) const FOOTER_BASELINE: i32 = LIBRA2_HEIGHT as i32 - 50;
pub(super) const MARGIN: i32 = 80;
pub(super) const FOOTER_PX: f32 = 28.0;
pub(super) const ROW_PX: f32 = 36.0;
/// Başlık bloğunun altı; listeler buradan başlar.
pub(super) const HEADER_BOTTOM: i32 = TOP + 175;
const TITLE_PX: f32 = 52.0;
const BORDER: u32 = 3;

pub(super) const BOLD: Style = Style {
    bold: true,
    italic: false,
    link: false,
};
pub(super) const PLAIN: Style = Style {
    bold: false,
    italic: false,
    link: false,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(&self, x: u32, y: u32) -> bool {
        let (x, y) = (x as i32, y as i32);
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    #[cfg(test)]
    pub fn center(&self) -> (u32, u32) {
        ((self.x + self.w / 2) as u32, (self.y + self.h / 2) as u32)
    }
}

/// Sayfalı dikey liste.
pub(super) struct List {
    pub top: i32,
    pub row_h: i32,
}

impl List {
    pub fn per_page(&self) -> usize {
        ((BOTTOM - self.top) / self.row_h).max(1) as usize
    }

    pub fn pages(&self, len: usize) -> usize {
        len.div_ceil(self.per_page()).max(1)
    }

    pub fn row_rect(&self, width: u32, row_on_page: usize) -> Rect {
        Rect {
            x: 0,
            y: self.top + row_on_page as i32 * self.row_h,
            w: width as i32,
            h: self.row_h,
        }
    }

    /// `page` sayfasında `y` yüksekliğindeki satırın listedeki sırası.
    pub fn row_at(&self, page: usize, len: usize, y: u32) -> Option<usize> {
        let y = y as i32;
        if y < self.top {
            return None;
        }
        let row = ((y - self.top) / self.row_h) as usize;
        let index = page * self.per_page() + row;
        (row < self.per_page() && index < len).then_some(index)
    }

    /// Görünen satırlar: (listedeki sıra, sayfadaki satır).
    pub fn visible(&self, page: usize, len: usize) -> impl Iterator<Item = (usize, usize)> {
        let start = page * self.per_page();
        (start..(start + self.per_page()).min(len))
            .enumerate()
            .map(|(row, index)| (index, row))
    }
}

/// Başlığın sağındaki "Geri" düğmesi.
pub(super) fn back_button(width: u32) -> Rect {
    Rect {
        x: width as i32 - MARGIN - 170,
        y: TOP,
        w: 170,
        h: 72,
    }
}

/// Başlıkta "Geri" düğmesinin solundaki `w` genişliğinde düğme.
pub(super) fn header_button(width: u32, w: i32) -> Rect {
    let back = back_button(width);
    Rect {
        x: back.x - 30 - w,
        w,
        ..back
    }
}

/// Ekran başlığı, alt başlık ve çizgi. `back` ise sağda "Geri" düğmesi.
pub(super) fn draw_header(t: &Text, canvas: &mut Canvas, title: &str, subtitle: &str, back: bool) {
    let width = canvas.width();
    let right = if back {
        back_button(width).x - 30
    } else {
        width as i32 - MARGIN
    };
    draw_header_until(t, canvas, title, subtitle, right);
    if back {
        draw_button(t, canvas, back_button(width), &tr(&Msg::Back), ROW_PX);
    }
}

/// Başlık ve alt başlık `right`'a kadar (sağında düğmeler olan ekranlar için).
pub(super) fn draw_header_until(
    t: &Text,
    canvas: &mut Canvas,
    title: &str,
    subtitle: &str,
    right: i32,
) {
    let width = canvas.width();
    let title = one_line(t, title, BOLD, TITLE_PX, (right - MARGIN) as f32);
    t.draw_plain(canvas, &title, Family::Ui, BOLD, MARGIN, TOP + 55, TITLE_PX);
    let subtitle = one_line(t, subtitle, PLAIN, ROW_PX, (right - MARGIN) as f32);
    t.draw_plain(
        canvas,
        &subtitle,
        Family::Ui,
        PLAIN,
        MARGIN,
        TOP + 115,
        ROW_PX,
    );
    canvas.fill_rect(MARGIN, TOP + 145, width - 2 * MARGIN as u32, BORDER, 0);
}

pub(super) fn draw_footer(t: &Text, canvas: &mut Canvas, msg: &Msg, x: i32) {
    t.draw_msg(canvas, msg, x, FOOTER_BASELINE, FOOTER_PX);
}

/// Çerçeveli, ortası yazılı düğme.
pub(super) fn draw_button(t: &Text, canvas: &mut Canvas, r: Rect, label: &str, px: f32) {
    canvas.fill_rect(r.x, r.y, r.w as u32, r.h as u32, 255);
    canvas.stroke_rect(r.x, r.y, r.w as u32, r.h as u32, BORDER);
    let w = t.width_plain(label, Family::Ui, PLAIN, px);
    let x = r.x + ((r.w as f32 - w) / 2.0) as i32;
    let baseline = r.y + r.h / 2 + (px * 0.35) as i32;
    t.draw_plain(canvas, label, Family::Ui, PLAIN, x, baseline, px);
}

/// Metnin tek satıra sığan kısmı; kesildiyse sonuna "…".
pub(super) fn one_line(t: &Text, text: &str, style: Style, px: f32, max_w: f32) -> String {
    let lines = t.wrap_plain(text, Family::Ui, style, px, max_w - px);
    let mut first = lines.first().cloned().unwrap_or_default();
    if lines.len() > 1 {
        first.push('…');
    }
    first
}
