//! PDF sayfasını ekrana yerleştirme: kenar kırpma, sığdırma, sütunlar,
//! yakınlaştırma ve ekranlara bölme; çizilen görüntü önbellekte tutulur.

use std::collections::HashMap;

use super::session::Session;
use super::settings::{PdfView, Settings};
use super::ui::{MARGIN, ROW_PX, TOP};
use super::{App, PdfFrame};
use crate::LIBRA2_HEIGHT;
use crate::canvas::Canvas;
use crate::i18n::Msg;
use crate::pdf::{Area, PageLayout, PdfDoc};

/// Sayfa görüntüsünün ekran kenarlarına uzaklığı (piksel).
const PAD: i32 = 8;
/// Sayfa görüntüsünün alt sınırı; altında alt bilgi satırı kalır.
const PAGE_BOTTOM: i32 = LIBRA2_HEIGHT as i32 - 80;
/// Kırpılmış görünüm, tüm sayfanın sığdığı ölçeğin en fazla bu katına büyür;
/// neredeyse boş sayfalarda (başlık, bölüm sonu) içerik aşırı büyümesin.
const MAX_CROP_ZOOM: f32 = 2.0;
/// Alt alta iki ekranın en az ne kadar örtüştüğü (ekran boyunun oranı); ekran
/// sınırında yarım kalan satır sonraki ekranda bütün görünür. Yan yana
/// ekranlar örtüşmez: satır yandaki ekranda devam eder, %200'de sayfa tam ikiye bölünür.
const OVERLAP: f32 = 0.05;

/// PDF sayfasının bir ekranı: sayfanın gösterilen bölümü (punto) ve çizim ölçeği.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Screen {
    pub area: Area,
    pub scale: f32,
}

/// Sayfanın okuma sırasıyla ekranları. Sayfa düzeni (kırpma, sütunlar) sayfa
/// başına bir kez hesaplanıp `layouts`'ta tutulur; hesaplanamazsa tüm sayfa gösterilir.
pub(super) fn page_screens(
    doc: &PdfDoc,
    layouts: &mut HashMap<usize, PageLayout>,
    settings: &Settings,
    width: u32,
    page: usize,
) -> Vec<Screen> {
    let Some((pw, ph)) = doc.page_size(page) else {
        return Vec::new();
    };
    let columns = settings.pdf_view == PdfView::Columns;
    if (settings.pdf_crop || columns) && !layouts.contains_key(&page) {
        match doc.page_layout(page) {
            Ok(l) => {
                layouts.insert(page, l);
            }
            Err(e) => log::warn!("no layout for pdf page {page}: {e:?}"),
        }
    }
    let full = Area {
        x: 0.0,
        y: 0.0,
        w: pw,
        h: ph,
    };
    // Sütun modunda sütunlar her durumda kırpılmış içerikten gelir. Birden çok
    // sütun bulunduysa sütun genişliğe sığar; sınır yok (sütun, içeriğin en az beşte biri).
    let (regions, capped) = match layouts.get(&page) {
        Some(l) if columns && l.columns.len() > 1 => (l.columns.clone(), false),
        Some(l) if settings.pdf_crop || columns => (vec![l.content], true),
        _ => (vec![full], true),
    };
    let avail_w = (width as i32 - 2 * PAD) as f32;
    let avail_h = (PAGE_BOTTOM - 2 * PAD) as f32;
    let fit_page = (avail_w / pw).min(avail_h / ph);
    let zoom = settings.pdf_zoom as f32 / 100.0;
    regions
        .iter()
        .flat_map(|r| {
            let fit = match settings.pdf_view {
                PdfView::Page => (avail_w / r.w).min(avail_h / r.h),
                PdfView::Width | PdfView::Columns => avail_w / r.w,
            };
            let fit = if capped {
                fit.min(fit_page * MAX_CROP_ZOOM)
            } else {
                fit
            };
            let scale = fit * zoom;
            tiles(*r, avail_w / scale, avail_h / scale)
                .into_iter()
                .map(move |area| Screen { area, scale })
        })
        .collect()
}

/// `r`'yi en fazla `w`×`h` boyutunda karolara böler; satır satır, soldan sağa.
fn tiles(r: Area, w: f32, h: f32) -> Vec<Area> {
    let xs = spans(r.x, r.w, w, 0.0);
    spans(r.y, r.h, h, OVERLAP)
        .into_iter()
        .flat_map(|(y, th)| xs.iter().map(move |&(x, tw)| Area { x, y, w: tw, h: th }))
        .collect()
}

/// `start`'tan başlayan `len` uzunluğunu `size` uzunluğunda, eşit aralıklı ve
/// en az `overlap` oranında örtüşen parçalara böler: (başlangıç, uzunluk).
fn spans(start: f32, len: f32, size: f32, overlap: f32) -> Vec<(f32, f32)> {
    // Yarım puntoluk fark, kayan nokta hatasıyla gereksiz ikinci ekran açmasın.
    if len <= size + 0.5 {
        return vec![(start, len)];
    }
    // Yarım puntodan küçük artık için ek parça açılmaz (kayan nokta hatası).
    let n = ((len - size - 0.5) / (size * (1.0 - overlap))).ceil() as usize + 1;
    let step = (len - size) / (n - 1) as f32;
    (0..n).map(|i| (start + i as f32 * step, size)).collect()
}

impl App {
    /// Okunan PDF ekranını çizer; aynı ekran için önbellekteki görüntüyü kullanır.
    pub(super) fn draw_pdf_page(&self, canvas: &mut Canvas, s: &Session) {
        let Some(doc) = s.pdf() else {
            return;
        };
        let Some(screen) = s.pages.get(s.page).and_then(|p| p.screen) else {
            self.draw_page_failed(canvas);
            return;
        };
        let mut frame = self.pdf_frame.borrow_mut();
        if !frame
            .as_ref()
            .is_some_and(|f| f.page == s.spine && f.screen == screen)
        {
            *frame = match doc.render_area(s.spine, screen.area, screen.scale) {
                Ok(image) => {
                    let avail_w = canvas.width() as i32 - 2 * PAD;
                    let avail_h = PAGE_BOTTOM - 2 * PAD;
                    Some(PdfFrame {
                        page: s.spine,
                        screen,
                        x: PAD + (avail_w - image.width as i32) / 2,
                        y: PAD + (avail_h - image.height as i32) / 2,
                        image,
                    })
                }
                Err(e) => {
                    log::error!("cannot render pdf page {}: {e:?}", s.spine);
                    None
                }
            };
        }
        match frame.as_ref() {
            Some(f) => canvas.blit(&f.image.pixels, f.image.width, f.image.height, f.x, f.y),
            None => self.draw_page_failed(canvas),
        }
    }

    fn draw_page_failed(&self, canvas: &mut Canvas) {
        self.text
            .draw_msg(canvas, &Msg::PdfPageFailed, MARGIN, TOP + 300, ROW_PX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_cover_the_length_with_overlap() {
        assert_eq!(spans(10.0, 100.0, 100.0, OVERLAP), vec![(10.0, 100.0)]);
        assert_eq!(
            spans(0.0, 200.0, 100.0, 0.0),
            vec![(0.0, 100.0), (100.0, 100.0)]
        );
        let s = spans(0.0, 250.0, 100.0, OVERLAP);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].0, 0.0);
        assert_eq!(s[2].0 + s[2].1, 250.0, "son parça sona dayanmalı");
        for w in s.windows(2) {
            assert!(w[0].0 + w[0].1 - w[1].0 >= 100.0 * OVERLAP - 1e-3, "{s:?}");
        }
    }
}
