//! Kütüphanenin kapak ızgarası (3×3). Kapaklar EPUB'ın kendi kapak
//! görselinden, PDF'te ilk sayfadan çizilir. Küçük resimler arka planda
//! üretilip veri klasöründe `kapaklar/<kimlik>.png` olarak saklanır (boş
//! dosya: kapağı yok); bellekte yalnızca görünen sayfanınkiler tutulur.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use super::jobs::Job;
use super::screens::LIBRARY;
use super::ui::{BOLD, BOTTOM, MARGIN, PLAIN, Rect, one_line};
use super::{App, View};
use crate::canvas::Canvas;
use crate::epub::{Book, decode_gray, fit};
use crate::event::Refresh;
use crate::i18n::tr;
use crate::library::Format;
use crate::pdf::PdfDoc;
use crate::text::Family;

const COLS: usize = 3;
const ROWS: usize = 3;
const GAP: i32 = 30;
/// Kapağın altındaki iki satır (başlık, ilerleme).
const CAPTION_H: i32 = 80;
const TITLE_PX: f32 = 26.0;
const NOTE_PX: f32 = 24.0;
const COVERS_DIR: &str = "kapaklar";

/// Gri tonlu küçük resim.
pub(super) struct Thumb {
    w: u32,
    h: u32,
    pixels: Vec<u8>,
}

/// Üretilecek kapaklar: (kimlik, dosya, biçim).
type Todo = Vec<(String, PathBuf, Format)>;
pub(super) type CoverJob = Job<Vec<(String, Option<Thumb>)>>;

pub(super) fn per_page() -> usize {
    COLS * ROWS
}

pub(super) fn pages(len: usize) -> usize {
    len.div_ceil(per_page()).max(1)
}

/// Sayfadaki `slot`. hücre (kapak ve altındaki yazı).
fn cell(width: u32, slot: usize) -> Rect {
    let w = (width as i32 - 2 * MARGIN - (COLS as i32 - 1) * GAP) / COLS as i32;
    let h = (BOTTOM - LIBRARY.top - (ROWS as i32 - 1) * GAP) / ROWS as i32;
    Rect {
        x: MARGIN + (slot % COLS) as i32 * (w + GAP),
        y: LIBRARY.top + (slot / COLS) as i32 * (h + GAP),
        w,
        h,
    }
}

/// Kapağın sığacağı kutu.
fn cover_box(width: u32) -> (u32, u32) {
    let c = cell(width, 0);
    (c.w as u32, (c.h - CAPTION_H) as u32)
}

/// `page` sayfasında (x, y)'deki kitabın görünen listedeki sırası.
pub(super) fn index_at(width: u32, page: usize, len: usize, x: u32, y: u32) -> Option<usize> {
    let slot = (0..per_page()).find(|&s| cell(width, s).contains(x, y))?;
    let index = page * per_page() + slot;
    (index < len).then_some(index)
}

fn file_of(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.png"))
}

/// Kaydedilmiş küçük resim: `None` henüz yok, `Some(None)` kitabın kapağı yok.
fn load(file: &Path) -> Option<Option<Thumb>> {
    let bytes = std::fs::read(file).ok()?;
    if bytes.is_empty() {
        return Some(None);
    }
    Some(decode_gray(&bytes, u32::MAX, u32::MAX, 1.0).map(|(w, h, pixels)| Thumb { w, h, pixels }))
}

fn save(file: &Path, thumb: Option<&Thumb>) {
    let bytes = match thumb {
        None => Vec::new(),
        Some(t) => {
            let Some(img) = image::GrayImage::from_raw(t.w, t.h, t.pixels.clone()) else {
                return;
            };
            let mut out = std::io::Cursor::new(Vec::new());
            if img.write_to(&mut out, image::ImageFormat::Png).is_err() {
                return;
            }
            out.into_inner()
        }
    };
    if let Err(e) = std::fs::write(file, bytes) {
        log::warn!("cannot save cover {}: {e}", file.display());
    }
}

/// Kitabın kapağını `max_w`×`max_h`'ye sığacak biçimde çizer.
fn make(path: &Path, format: Format, max_w: u32, max_h: u32) -> Option<Thumb> {
    match format {
        Format::Epub => {
            let book = Book::open(path).ok()?;
            let bytes = book.image_bytes(book.cover.as_deref()?)?;
            let (w, h, pixels) = decode_gray(&bytes, max_w, max_h, 4.0)?;
            Some(Thumb { w, h, pixels })
        }
        Format::Pdf => {
            let doc = PdfDoc::open(path).ok()?;
            let (pw, ph) = doc.page_size(0)?;
            let (w, _) = fit(pw as u32, ph as u32, max_w, max_h, 4.0);
            let img = doc.render(0, w as f32 / pw).ok()?;
            Some(Thumb {
                w: img.width,
                h: img.height,
                pixels: img.pixels,
            })
        }
    }
}

impl App {
    fn grid_shown(&self) -> Option<usize> {
        match self.view {
            View::Library { page }
                if self.settings.library_view == super::shelf::LibraryView::Covers =>
            {
                Some(page)
            }
            _ => None,
        }
    }

    /// Izgarada görünen kitapların kapaklarını diskten yükler; eksikleri
    /// arka planda üretmeye başlar (bir iş bitmeden yenisi başlamaz).
    pub(super) fn request_covers(&mut self) {
        let Some(page) = self.grid_shown() else {
            return;
        };
        let books = self.visible_books();
        let shown: Vec<usize> = books
            .iter()
            .skip(page * per_page())
            .take(per_page())
            .copied()
            .collect();
        let ids: Vec<&str> = shown.iter().map(|&i| self.library[i].id.as_str()).collect();
        self.covers.retain(|id, _| ids.contains(&id.as_str()));
        let dir = self.store.dir().join(COVERS_DIR);
        let mut todo: Todo = Vec::new();
        for &i in &shown {
            let e = &self.library[i];
            if self.covers.contains_key(&e.id) {
                continue;
            }
            if !e.readable {
                self.covers.insert(e.id.clone(), None);
            } else if let Some(thumb) = load(&file_of(&dir, &e.id)) {
                self.covers.insert(e.id.clone(), thumb);
            } else {
                todo.push((e.id.clone(), e.path.clone(), e.format));
            }
        }
        if todo.is_empty() || self.cover_job.is_some() {
            return;
        }
        let (max_w, max_h) = cover_box(self.portrait_width);
        self.cover_job = Some(Job::spawn(move |cancel: &AtomicBool| {
            if let Err(e) = std::fs::create_dir_all(&dir) {
                log::warn!("cannot create {}: {e}", dir.display());
            }
            let mut out = Vec::new();
            for (id, path, format) in todo {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let thumb = make(&path, format, max_w, max_h);
                save(&file_of(&dir, &id), thumb.as_ref());
                out.push((id, thumb));
            }
            out
        }));
    }

    /// Kapak işi bittiyse kapakları alır, sırada bekleyen varsa yenisini başlatır.
    pub(super) fn poll_covers(&mut self) -> Option<Refresh> {
        let done = self.cover_job.as_ref()?.poll()?;
        self.cover_job = None;
        self.covers.extend(done.unwrap_or_default());
        self.request_covers();
        self.grid_shown()?;
        Some(self.refresh(false))
    }

    pub(super) fn draw_grid(&self, canvas: &mut Canvas, books: &[usize], page: usize) {
        let t = &self.text;
        let (box_w, box_h) = cover_box(canvas.width());
        for (slot, &i) in books
            .iter()
            .skip(page * per_page())
            .take(per_page())
            .enumerate()
        {
            let e = &self.library[i];
            let c = cell(canvas.width(), slot);
            match self.covers.get(&e.id) {
                Some(Some(th)) => {
                    // PDF sayfası yuvarlamayla kutudan bir piksel taşabilir.
                    let x = c.x + (box_w as i32 - th.w as i32).max(0) / 2;
                    let y = c.y + (box_h as i32 - th.h as i32).max(0);
                    canvas.blit(&th.pixels, th.w, th.h, x, y);
                    canvas.stroke_rect(x, y, th.w, th.h, 1);
                }
                // Kapağı yok ya da henüz çizilmedi: başlığı yazılı düz kapak.
                _ => {
                    let (w, h) = fit(2, 3, box_w, box_h, f32::MAX);
                    let x = c.x + (box_w - w) as i32 / 2;
                    let y = c.y + (box_h - h) as i32;
                    canvas.stroke_rect(x, y, w, h, 3);
                    let max_w = w as f32 - 40.0;
                    let lines = t.wrap_plain(&e.display_title(), Family::Ui, BOLD, 30.0, max_w);
                    for (n, line) in lines.iter().take(5).enumerate() {
                        t.draw_plain(
                            canvas,
                            line,
                            Family::Ui,
                            BOLD,
                            x + 20,
                            y + 60 + n as i32 * 42,
                            30.0,
                        );
                    }
                }
            }
            let max_w = c.w as f32;
            let title = one_line(t, &e.display_title(), BOLD, TITLE_PX, max_w);
            let base = c.y + box_h as i32;
            t.draw_plain(canvas, &title, Family::Ui, BOLD, c.x, base + 34, TITLE_PX);
            let note = if !e.readable {
                tr(&crate::i18n::Msg::Unreadable)
            } else {
                let percent = self.progress.get(&e.id).copied().filter(|&p| p > 0);
                let mut parts: Vec<String> = percent
                    .map(|p| tr(&crate::i18n::Msg::Percent(p)))
                    .into_iter()
                    .collect();
                parts.extend(self.status_note(&e.id).map(|m| tr(&m)));
                parts.join(" · ")
            };
            let note = one_line(t, &note, PLAIN, NOTE_PX, max_w);
            t.draw_plain(canvas, &note, Family::Ui, PLAIN, c.x, base + 70, NOTE_PX);
        }
    }
}
