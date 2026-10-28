//! EPUB görselleri: sayfada genişliğe ve sayfa yüksekliğine sığdırılmış hâlde
//! çizilir; uzun basınca tam ekran açılır, dokununca kapanır (kısa dokunma
//! sayfa çevirir: sayfayı kaplayan kapakta başka türlü çevrilemezdi). JPEG ve
//! PNG desteklenir; diğer biçimlerin yerinde alt metin ya da yer tutucu kalır.
//! Çözülen görseller oturum boyunca birkaç tane önbellekte tutulur.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::session::{Doc, PlacedLine, Session};
use super::{App, View};
use crate::canvas::Canvas;
use crate::epub::{ImageRef, decode_gray, fit};
use crate::event::{Event, Refresh};
use crate::{LIBRA2_HEIGHT, LIBRA2_WIDTH};

/// Küçük görseller en çok bu kat büyütülür (sayfada); tam ekranda daha fazla.
pub(super) const UPSCALE: f32 = 2.0;
const FULL_SCREEN_UPSCALE: f32 = 4.0;
/// Önbellekteki en fazla görsel sayısı (cihaz belleği sınırlı).
const CACHED: usize = 6;
const FULL_SCREEN_PAD: u32 = 20;

type Key = (String, u32, u32);

#[derive(Default)]
pub(super) struct ImageCache(RefCell<HashMap<Key, Rc<Vec<u8>>>>);

impl Session {
    /// `img`'nin `w`×`h` boyutunda gri pikselleri (önbellekten ya da çözülerek).
    fn image_pixels(&self, img: &ImageRef, w: u32, h: u32) -> Option<Rc<Vec<u8>>> {
        let key = (img.path.clone(), w, h);
        if let Some(p) = self.images.0.borrow().get(&key) {
            return Some(p.clone());
        }
        let Doc::Epub(book) = &self.doc else {
            return None;
        };
        let started = std::time::Instant::now();
        let bytes = book.image_bytes(&img.path)?;
        let (dw, dh, pixels) = decode_gray(&bytes, w, h, f32::MAX)?;
        if (dw, dh) != (w, h) {
            return None;
        }
        log::info!(
            "image {} {w}x{h} decoded in {:?}",
            img.path,
            started.elapsed()
        );
        let pixels = Rc::new(pixels);
        let mut cache = self.images.0.borrow_mut();
        if cache.len() >= CACHED {
            cache.clear();
        }
        cache.insert(key, pixels.clone());
        Some(pixels)
    }
}

impl App {
    /// Sayfadaki görsel satırını yatayda ortalayarak çizer.
    pub(super) fn draw_book_image(
        &self,
        canvas: &mut Canvas,
        s: &Session,
        placed: &PlacedLine,
    ) -> bool {
        let Some(img) = s.blocks.get(placed.block).and_then(|b| b.image.as_ref()) else {
            return false;
        };
        let h = placed.height.max(1) as u32;
        let w = ((img.width as f32 * h as f32 / img.height as f32).round() as u32).max(1);
        if let Some(pixels) = s.image_pixels(img, w, h) {
            let x = (self.width as i32 - w as i32) / 2;
            canvas.blit(&pixels, w, h, x, placed.top);
        }
        true
    }

    /// Okunan sayfada dokunulan görselin bloğu.
    pub(super) fn image_at(&self, x: u32, y: u32) -> Option<usize> {
        let s = self.session.as_ref()?;
        let page = s.pages.get(s.page)?;
        let y = y as i32;
        page.lines
            .iter()
            .find(|l| {
                y >= l.top
                    && y < l.top + l.height
                    && s.blocks.get(l.block).is_some_and(|b| b.image.is_some())
            })
            .map(|l| l.block)
            .filter(|_| x < self.width)
    }

    pub(super) fn handle_image_view(&mut self, event: Event) -> Option<Refresh> {
        matches!(event, Event::Tap { .. } | Event::Key(_)).then(|| {
            self.view = View::Reading;
            self.refresh(true)
        })
    }

    /// Görsel tam ekran, ortalanmış.
    pub(super) fn draw_image_view(&self, canvas: &mut Canvas, block: usize) {
        let Some(s) = &self.session else {
            return;
        };
        let Some(img) = s.blocks.get(block).and_then(|b| b.image.as_ref()) else {
            return;
        };
        let (w, h) = fit(
            img.width,
            img.height,
            LIBRA2_WIDTH - 2 * FULL_SCREEN_PAD,
            LIBRA2_HEIGHT - 2 * FULL_SCREEN_PAD,
            FULL_SCREEN_UPSCALE,
        );
        if let Some(pixels) = s.image_pixels(img, w, h) {
            let x = (canvas.width() as i32 - w as i32) / 2;
            let y = (canvas.height() as i32 - h as i32) / 2;
            canvas.blit(&pixels, w, h, x, y);
        }
    }
}
