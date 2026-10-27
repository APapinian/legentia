//! PDF sayfasını ekrana yerleştirme: kenar kırpma, sığdırma, önbellek.

use super::session::Session;
use super::ui::{MARGIN, ROW_PX, TOP};
use super::{App, PdfFrame};
use crate::LIBRA2_HEIGHT;
use crate::canvas::Canvas;
use crate::i18n::Msg;
use crate::pdf::{Area, PdfDoc, PdfError};

/// Sayfa görüntüsünün ekran kenarlarına uzaklığı (piksel).
const PAD: i32 = 8;
/// Sayfa görüntüsünün alt sınırı; altında alt bilgi satırı kalır.
const PAGE_BOTTOM: i32 = LIBRA2_HEIGHT as i32 - 80;
/// Kırpılmış görünüm, tüm sayfanın sığdığı ölçeğin en fazla bu katına büyür;
/// neredeyse boş sayfalarda (başlık, bölüm sonu) içerik aşırı büyümesin.
const MAX_CROP_ZOOM: f32 = 2.0;

impl App {
    /// Okunan PDF sayfasını çizer; aynı sayfa ve ayar için önbellekteki görüntüyü kullanır.
    pub(super) fn draw_pdf_page(&self, canvas: &mut Canvas, s: &Session) {
        let Some(doc) = s.pdf() else {
            return;
        };
        let crop = self.settings.pdf_crop;
        let mut frame = self.pdf_frame.borrow_mut();
        if !frame
            .as_ref()
            .is_some_and(|f| f.page == s.page && f.crop == crop)
        {
            *frame = match self.render_frame(doc, s.page, crop, canvas.width()) {
                Ok(f) => Some(f),
                Err(e) => {
                    log::error!("cannot render pdf page {}: {e:?}", s.page);
                    None
                }
            };
        }
        match frame.as_ref() {
            Some(f) => canvas.blit(&f.image.pixels, f.image.width, f.image.height, f.x, f.y),
            None => self
                .text
                .draw_msg(canvas, &Msg::PdfPageFailed, MARGIN, TOP + 300, ROW_PX),
        }
    }

    fn render_frame(
        &self,
        doc: &PdfDoc,
        page: usize,
        crop: bool,
        width: u32,
    ) -> Result<PdfFrame, PdfError> {
        let (pw, ph) = doc.page_size(page).ok_or(PdfError::PageMissing(page))?;
        let area = if crop {
            self.content_area(doc, page)?
        } else {
            Area {
                x: 0.0,
                y: 0.0,
                w: pw,
                h: ph,
            }
        };
        let avail_w = (width as i32 - 2 * PAD) as f32;
        let avail_h = (PAGE_BOTTOM - 2 * PAD) as f32;
        let fit_page = (avail_w / pw).min(avail_h / ph);
        let scale = (avail_w / area.w)
            .min(avail_h / area.h)
            .min(fit_page * MAX_CROP_ZOOM);
        let image = doc.render_area(page, area, scale)?;
        let x = PAD + ((avail_w - image.width as f32) / 2.0) as i32;
        let y = PAD + ((avail_h - image.height as f32) / 2.0) as i32;
        Ok(PdfFrame {
            page,
            crop,
            image,
            x,
            y,
        })
    }

    /// Kırpılmış içerik alanı; sayfa başına bir kez hesaplanır.
    fn content_area(&self, doc: &PdfDoc, page: usize) -> Result<Area, PdfError> {
        if let Some(area) = self.pdf_areas.borrow().get(&page) {
            return Ok(*area);
        }
        let area = doc.content_area(page)?;
        self.pdf_areas.borrow_mut().insert(page, area);
        Ok(area)
    }
}
