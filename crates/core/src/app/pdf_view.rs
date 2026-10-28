//! PDF sayfasını ekrana yerleştirme: kenar kırpma, sığdırma, sütunlar,
//! yakınlaştırma ve ekranlara bölme; çizilen görüntü önbellekte tutulur.

use std::collections::HashMap;

use super::session::Session;
use super::settings::{PdfView, Settings};
use super::ui::{MARGIN, ROW_PX, Rect, TOP};
use super::{App, PdfFrame};
use crate::canvas::Canvas;
use crate::i18n::Msg;
use crate::pdf::{Area, PageLayout, PdfDoc};

/// Sayfa görüntüsünün ekran kenarlarına uzaklığı (piksel).
const PAD: i32 = 8;
/// Sayfa görüntüsünün alt sınırı; altında alt bilgi satırı kalır.
fn page_bottom(height: u32) -> i32 {
    height as i32 - 80
}
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

/// Ekranın görüntüsünün sol üst köşesi; görüntü sayfa alanının ortasına yerleşir.
pub(super) fn screen_origin(screen: &Screen, size: (u32, u32)) -> (i32, i32) {
    let w = (screen.area.w * screen.scale).round() as i32;
    let h = (screen.area.h * screen.scale).round() as i32;
    let avail_w = size.0 as i32 - 2 * PAD;
    let avail_h = page_bottom(size.1) - 2 * PAD;
    (PAD + (avail_w - w) / 2, PAD + (avail_h - h) / 2)
}

/// Ekran noktasının sayfadaki yeri (punto).
pub(super) fn to_page(screen: &Screen, size: (u32, u32), x: u32, y: u32) -> (f32, f32) {
    let (ox, oy) = screen_origin(screen, size);
    (
        screen.area.x + (x as i32 - ox) as f32 / screen.scale,
        screen.area.y + (y as i32 - oy) as f32 / screen.scale,
    )
}

/// Sayfa alanının (punto) ekrandaki dikdörtgeni.
pub(super) fn to_screen(screen: &Screen, size: (u32, u32), a: Area) -> Rect {
    let (ox, oy) = screen_origin(screen, size);
    let x = ox + ((a.x - screen.area.x) * screen.scale).floor() as i32;
    let y = oy + ((a.y - screen.area.y) * screen.scale).floor() as i32;
    Rect {
        x,
        y,
        w: (a.w * screen.scale).ceil() as i32 + 1,
        h: (a.h * screen.scale).ceil() as i32 + 1,
    }
}

/// Sayfanın okuma sırasıyla ekranları. Sayfa düzeni (kırpma, sütunlar) sayfa
/// başına bir kez hesaplanıp `layouts`'ta tutulur; hesaplanamazsa tüm sayfa gösterilir.
pub(super) fn page_screens(
    doc: &PdfDoc,
    layouts: &mut HashMap<usize, PageLayout>,
    settings: &Settings,
    size: (u32, u32),
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
    let avail_w = (size.0 as i32 - 2 * PAD) as f32;
    let avail_h = (page_bottom(size.1) - 2 * PAD) as f32;
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

/// Sayfa ekranının görüntüsü: önbellekte aynısı varsa o, yoksa çizilir.
fn render_frame<'a>(
    cache: &'a std::cell::RefCell<Option<PdfFrame>>,
    doc: &PdfDoc,
    page: usize,
    screen: Screen,
    area: (u32, u32),
    shift: i32,
) -> std::cell::RefMut<'a, Option<PdfFrame>> {
    let mut frame = cache.borrow_mut();
    if !frame
        .as_ref()
        .is_some_and(|f| f.page == page && f.screen == screen)
    {
        *frame = match doc.render_area(page, screen.area, screen.scale) {
            Ok(image) => {
                let (x, y) = screen_origin(&screen, area);
                Some(PdfFrame {
                    page,
                    screen,
                    x: x + shift,
                    y,
                    image,
                })
            }
            Err(e) => {
                log::error!("cannot render pdf page {page}: {e:?}");
                None
            }
        };
    }
    frame
}

fn blit_frame(canvas: &mut Canvas, frame: &Option<PdfFrame>) -> bool {
    match frame {
        Some(f) => {
            canvas.blit(&f.image.pixels, f.image.width, f.image.height, f.x, f.y);
            true
        }
        None => false,
    }
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
        let area = s.pdf_size((canvas.width(), canvas.height()));
        let left = render_frame(&self.pdf_frame, doc, s.spine, screen, area, 0);
        if !blit_frame(canvas, &left) {
            self.draw_page_failed(canvas);
        }
        // Yatayda sağdaki sayfa, ekranın sağ yarısında.
        if let Some(right) = s.right {
            let frame = render_frame(
                &self.pdf_right,
                doc,
                s.spine + 1,
                right,
                area,
                area.0 as i32,
            );
            blit_frame(canvas, &frame);
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
