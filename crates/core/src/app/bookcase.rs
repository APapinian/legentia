//! Kütüphanenin raf görünümü: her klasör bir raf, kitaplar rafta sırtları
//! görünecek biçimde dizilir, ad sırtta aşağıdan yukarı yazılır. Rafa
//! sığmayan kitaplar aynı rafın alt sırasına geçer; sayfa çevrildikçe alttaki
//! raflar gelir. Kitabı rafa koymak klasöre taşımaktır (kitap işlemleri).

use std::path::Path;

use super::App;
use super::screens::LIBRARY;
use super::ui::{BOLD, MARGIN, PLAIN, Rect, one_line};
use crate::canvas::Canvas;
use crate::i18n::{Msg, tr};
use crate::text::Family;

/// Sayfadaki sıra sayısı ve bir sıranın yüksekliği (raf adı, sırtlar, tahta).
const ROWS: usize = 3;
const ROW_H: i32 = 380;
const NAME_H: i32 = 56;
const SPINE_H: i32 = 280;
const SPINE_W: i32 = 72;
const SPINE_GAP: i32 = 6;
const PLANK_H: i32 = 14;
const SPINE_PX: f32 = 25.0;
/// Sırtların zemini; kitaplar birbirinden ayrılsın diye sırayla.
const SHADES: [u8; 3] = [255, 225, 195];

/// Bir raf sırası: rafın ilk sırasıysa raf adı, kitaplar (görünen listedeki sıraları).
struct Row {
    name: Option<String>,
    books: Vec<usize>,
}

fn per_row(width: u32) -> usize {
    ((width as i32 - 2 * MARGIN + SPINE_GAP) / (SPINE_W + SPINE_GAP)).max(1) as usize
}

/// Kimliğe bağlı küçük sayı: sırtın yüksekliği ve rengi kitaptan kitaba değişsin.
fn variant(id: &str) -> usize {
    id.bytes()
        .fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize))
}

/// `page` sayfasındaki `row`. sıranın `slot`. sırtı.
fn spine_rect(row: usize, slot: usize, id: &str) -> Rect {
    let h = SPINE_H - (variant(id) % 5) as i32 * 14;
    let bottom = LIBRARY.top + row as i32 * ROW_H + NAME_H + SPINE_H;
    Rect {
        x: MARGIN + slot as i32 * (SPINE_W + SPINE_GAP),
        y: bottom - h,
        w: SPINE_W,
        h,
    }
}

/// `text`'i aşağıdan yukarı okunacak biçimde (90° sola döndürülmüş) `r`'nin
/// içine en çok iki satır yazar.
fn draw_rotated(app: &App, canvas: &mut Canvas, text: &str, r: Rect) {
    let t = &app.text;
    let max_w = (r.h - 30) as f32;
    let mut lines = t.wrap_plain(text, Family::Ui, PLAIN, SPINE_PX, max_w);
    if lines.len() > 2 {
        lines[1] = one_line(t, &lines[1..].join(" "), PLAIN, SPINE_PX, max_w);
        lines.truncate(2);
    }
    let line_h = (SPINE_PX * 1.2) as u32;
    let w = lines
        .iter()
        .map(|l| t.width_plain(l, Family::Ui, PLAIN, SPINE_PX).ceil() as u32)
        .max()
        .unwrap_or(0)
        .max(1);
    let h = line_h * lines.len() as u32 + line_h / 4;
    let mut tmp = Canvas::new(w, h);
    for (i, line) in lines.iter().enumerate() {
        let baseline = (i as u32 * line_h) as i32 + (SPINE_PX * 0.95) as i32;
        t.draw_plain(&mut tmp, line, Family::Ui, PLAIN, 0, baseline, SPINE_PX);
    }
    // Döndürülmüş metin: sırtın altından başlar, enine ortalanır.
    let x0 = r.x + (r.w - h as i32) / 2;
    let y0 = r.y + r.h - 15;
    let px = tmp.pixels();
    for y in 0..h {
        for x in 0..w {
            let g = px[(y * w + x) as usize];
            if g < 255 {
                canvas.darken(x0 + y as i32, y0 - x as i32, g);
            }
        }
    }
}

impl App {
    fn shelf_rows(&self, books: &[usize]) -> Vec<Row> {
        let folder = |i: usize| self.library[books[i]].path.parent().map(Path::to_path_buf);
        let mut folders: Vec<_> = (0..books.len()).filter_map(folder).collect();
        folders.sort_by(|a, b| {
            // Ana klasör önce, sonra adına göre.
            (a != &self.root)
                .cmp(&(b != &self.root))
                .then_with(|| crate::collate::compare(&a.to_string_lossy(), &b.to_string_lossy()))
        });
        folders.dedup();
        let per = per_row(self.width);
        let mut rows = Vec::new();
        for f in folders {
            let mine: Vec<usize> = (0..books.len())
                .filter(|&i| folder(i).as_ref() == Some(&f))
                .collect();
            let name = if f == self.root {
                tr(&Msg::LibraryRoot)
            } else {
                f.strip_prefix(&self.root)
                    .unwrap_or(&f)
                    .to_string_lossy()
                    .into_owned()
            };
            for (n, chunk) in mine.chunks(per).enumerate() {
                rows.push(Row {
                    name: (n == 0).then(|| format!("{name} ({})", mine.len())),
                    books: chunk.to_vec(),
                });
            }
        }
        rows
    }

    pub(super) fn bookcase_pages(&self, books: &[usize]) -> usize {
        self.shelf_rows(books).len().div_ceil(ROWS).max(1)
    }

    /// (x, y)'deki sırtın kitabı (görünen listedeki sırası). Kısa sırtın
    /// üstündeki boşluğa basmak da o kitabı seçer.
    pub(super) fn bookcase_index_at(
        &self,
        books: &[usize],
        page: usize,
        x: u32,
        y: u32,
    ) -> Option<usize> {
        let rows = self.shelf_rows(books);
        for (r, row) in rows.iter().skip(page * ROWS).take(ROWS).enumerate() {
            for (slot, &i) in row.books.iter().enumerate() {
                // Boş kimlikle sırt en uzun boyundadır.
                if spine_rect(r, slot, "").contains(x, y) {
                    return Some(i);
                }
            }
        }
        None
    }

    pub(super) fn draw_bookcase(&self, canvas: &mut Canvas, books: &[usize], page: usize) {
        let t = &self.text;
        let rows = self.shelf_rows(books);
        let plank_w = (canvas.width() as i32 - 2 * MARGIN + 40) as u32;
        for (r, row) in rows.iter().skip(page * ROWS).take(ROWS).enumerate() {
            let top = LIBRARY.top + r as i32 * ROW_H;
            if let Some(name) = &row.name {
                let name = one_line(
                    t,
                    name,
                    BOLD,
                    30.0,
                    (canvas.width() as i32 - 2 * MARGIN) as f32,
                );
                t.draw_plain(canvas, &name, Family::Ui, BOLD, MARGIN, top + 38, 30.0);
            }
            for (slot, &i) in row.books.iter().enumerate() {
                let e = &self.library[books[i]];
                let s = spine_rect(r, slot, &e.id);
                let shade = SHADES[variant(&e.id) / 5 % SHADES.len()];
                canvas.fill_rect(s.x, s.y, s.w as u32, s.h as u32, shade);
                canvas.stroke_rect(s.x, s.y, s.w as u32, s.h as u32, 2);
                // Bitmiş kitabın sırtında üstte koyu bant.
                if self.status_of(&e.id) == super::shelf::ReadStatus::Finished {
                    canvas.fill_rect(s.x, s.y, s.w as u32, 12, 0);
                }
                draw_rotated(self, canvas, &e.display_title(), s);
            }
            let plank_y = top + NAME_H + SPINE_H;
            canvas.fill_rect(MARGIN - 20, plank_y, plank_w, PLANK_H as u32, 70);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spines_fit_the_screen_width() {
        let per = per_row(crate::LIBRA2_WIDTH);
        assert_eq!(per, 14);
        let last = spine_rect(0, per - 1, "");
        assert!(last.x + last.w <= crate::LIBRA2_WIDTH as i32 - MARGIN);
        let bottom = spine_rect(ROWS - 1, 0, "");
        assert!(bottom.y + bottom.h + PLANK_H <= super::super::ui::BOTTOM);
    }
}
