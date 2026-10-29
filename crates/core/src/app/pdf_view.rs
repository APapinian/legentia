//! PDF sayfasını ekrana yerleştirme: kenar kırpma, sığdırma, sütunlar,
//! yakınlaştırma ve ekranlara bölme; çizilen görüntü önbellekte tutulur.

use std::collections::HashMap;

use super::session::Session;
use super::settings::{PdfView, Settings};
use super::ui::{MARGIN, ROW_PX, Rect, TOP};
use super::{App, PdfFrame};
use crate::canvas::Canvas;
use crate::i18n::Msg;
use crate::pdf::{Area, GrayImage, PageLayout, PdfDoc};

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
    if settings.pdf_view == PdfView::Strip {
        return strip_screens(doc, size, page);
    }
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
                PdfView::Width | PdfView::Columns | PdfView::Strip => avail_w / r.w,
            };
            let fit = if capped {
                fit.min(fit_page * MAX_CROP_ZOOM)
            } else {
                fit
            };
            let scale = fit * zoom;
            tiles(*r, avail_w / scale, avail_h / scale, settings.rtl)
                .into_iter()
                .map(move |area| Screen { area, scale })
        })
        .collect()
}

/// Webtoon: bütün sayfalar genişliğe sığdırılmış ve alt alta dizilmiş
/// sayılır; ekranlar bu şeridi eşit adımlarla böler. Sayfa, üst kenarı
/// kendisine düşen ekranları taşır (ekran sayfa sonunu aşabilir; aşan kısım
/// çizilirken sonraki sayfalardan gelir). Hiç ekran düşmeyen kısa sayfa
/// kendi başından bir ekran alır. Yakınlaştırma ve kırpma kullanılmaz.
fn strip_screens(doc: &PdfDoc, size: (u32, u32), page: usize) -> Vec<Screen> {
    let avail_w = (size.0 as i32 - 2 * PAD) as f32;
    let avail_h = (page_bottom(size.1) - 2 * PAD) as f32;
    let height = |i: usize| doc.page_size(i).map_or(0.0, |(w, h)| h * avail_w / w);
    let start: f32 = (0..page).map(height).sum();
    let Some((pw, _)) = doc.page_size(page) else {
        return Vec::new();
    };
    let end = start + height(page);
    let step = avail_h * (1.0 - OVERLAP);
    let scale = avail_w / pw;
    let mut tops: Vec<f32> = ((start / step - 1e-3).ceil().max(0.0) as usize..)
        .map(|k| k as f32 * step)
        .take_while(|&t| t < end - 0.5)
        .collect();
    if tops.is_empty() {
        tops.push(start);
    }
    tops.into_iter()
        .map(|t| Screen {
            area: Area {
                x: 0.0,
                y: (t - start) / scale,
                w: pw,
                h: avail_h / scale,
            },
            scale,
        })
        .collect()
}

/// Ekranın görüntüsü. Webtoon ekranı sayfa sonunu aşıyorsa kalan yükseklik
/// sonraki sayfaların başından, genişliğe sığdırılarak alta eklenir.
fn render_screen(
    doc: &PdfDoc,
    page: usize,
    screen: Screen,
) -> Result<GrayImage, crate::pdf::PdfError> {
    let ph = doc.page_size(page).map_or(0.0, |(_, h)| h);
    let a = screen.area;
    if a.y + a.h <= ph + 0.5 {
        return doc.render_area(page, a, screen.scale);
    }
    let width = (a.w * screen.scale).round().max(1.0) as u32;
    let height = (a.h * screen.scale).round().max(1.0) as u32;
    let mut out = GrayImage {
        width,
        height,
        pixels: vec![255; (width * height) as usize],
    };
    let (mut p, mut area, mut scale) = (page, Area { h: ph - a.y, ..a }, screen.scale);
    let mut y = 0u32;
    loop {
        if area.h > 0.5 {
            let img = doc.render_area(p, area, scale)?;
            let w = img.width.min(width) as usize;
            for row in 0..img.height.min(height - y) {
                let src = (row * img.width) as usize;
                let dst = ((y + row) * width) as usize;
                out.pixels[dst..dst + w].copy_from_slice(&img.pixels[src..src + w]);
            }
            y += img.height;
        }
        p += 1;
        let Some((pw, ph)) = doc.page_size(p).filter(|_| y < height) else {
            break;
        };
        scale = width as f32 / pw;
        area = Area {
            x: 0.0,
            y: 0.0,
            w: pw,
            h: ((height - y) as f32 / scale).min(ph),
        };
    }
    Ok(out)
}

/// `r`'yi en fazla `w`×`h` boyutunda karolara böler; satır satır, soldan
/// sağa (`rtl`'de sağdan sola).
fn tiles(r: Area, w: f32, h: f32, rtl: bool) -> Vec<Area> {
    let mut xs = spans(r.x, r.w, w, 0.0);
    if rtl {
        xs.reverse();
    }
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
    let (x, y) = screen_origin(&screen, area);
    // Yer de karşılaştırılır: sağdan sola açılınca yan yana sayfalar yer değiştirir.
    if !frame
        .as_ref()
        .is_some_and(|f| f.page == page && f.screen == screen && f.x == x + shift)
    {
        *frame = match render_screen(doc, page, screen) {
            Ok(image) => Some(PdfFrame {
                page,
                screen,
                x: x + shift,
                y,
                image,
            }),
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
        // Sağdan sola kitapta yan yana sayfaların ilki sağda.
        let (first, second) = if self.settings.rtl {
            (area.0 as i32, 0)
        } else {
            (0, area.0 as i32)
        };
        let left = render_frame(&self.pdf_frame, doc, s.spine, screen, area, first);
        if !blit_frame(canvas, &left) {
            self.draw_page_failed(canvas);
        }
        // Yatayda sağdaki sayfa, ekranın sağ yarısında.
        if let Some(right) = s.right {
            let frame = render_frame(&self.pdf_right, doc, s.spine + 1, right, area, second);
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

    /// Siyah ve beyaz iki sayfalık çizgi roman (100×150 piksel sayfalar).
    fn two_page_comic() -> PdfDoc {
        let black = crate::epub::testutil::png(100, 150, 0);
        let white = crate::epub::testutil::png(100, 150, 255);
        let cbz = crate::epub::testutil::zip_of_bytes(&[("1.png", &black), ("2.png", &white)]);
        let dir = crate::storage::temp_store("webtoon").dir().to_path_buf();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("w.cbz"), cbz).unwrap();
        PdfDoc::open(&dir.join("w.cbz")).unwrap()
    }

    #[test]
    fn webtoon_screens_run_across_pages() {
        let doc = two_page_comic();
        let size = (crate::LIBRA2_WIDTH, crate::LIBRA2_HEIGHT);
        let first = strip_screens(&doc, size, 0);
        let second = strip_screens(&doc, size, 1);
        // Şerit 2 × 1872 piksel, adım ekranın %95'i: ekranlar 0, 1505, 3010'da.
        assert_eq!((first.len(), second.len()), (2, 1));
        let step = first[1].area.y * first[1].scale;
        assert!((step - 1584.0 * (1.0 - OVERLAP)).abs() < 1.0, "{step}");
        let top = 1872.0 + second[0].area.y * second[0].scale;
        assert!((top - 2.0 * step).abs() < 1.0, "{top}");
        // Sınırı aşan ekranın üstü birinci (siyah), altı ikinci (beyaz) sayfadan.
        let img = render_screen(&doc, 0, first[1]).unwrap();
        assert_eq!(img.height, 1584);
        let at = |y: u32| img.pixels[(y * img.width + img.width / 2) as usize];
        assert!(at(10) < 40 && at(300) < 40, "siyah sayfa sonu");
        assert!(at(400) > 215 && at(1500) > 215, "beyaz sayfa başı");
    }

    #[test]
    fn right_to_left_tiles_start_on_the_right() {
        let r = Area {
            x: 0.0,
            y: 0.0,
            w: 200.0,
            h: 100.0,
        };
        let xs = |rtl| -> Vec<f32> { tiles(r, 100.0, 100.0, rtl).iter().map(|a| a.x).collect() };
        assert_eq!(xs(false), [0.0, 100.0]);
        assert_eq!(xs(true), [100.0, 0.0]);
    }

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
