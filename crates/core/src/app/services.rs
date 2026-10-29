//! Araçlar → Hesaplar ve servisler: DeepL anahtarı, WebDAV, Wallabag ve
//! Dropbox hesapları tek ekranda.

use super::ui::{HEADER_BOTTOM, MARGIN, ROW_PX, Rect, back_button, draw_button, draw_header};
use super::{App, View};
use crate::canvas::Canvas;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};

const BUTTON_H: i32 = 110;

impl App {
    /// Servisler ekranının `i`. satırı (DeepL, WebDAV, Wallabag, Dropbox).
    pub(super) fn service_row(&self, i: i32) -> Rect {
        Rect {
            x: MARGIN,
            y: HEADER_BOTTOM + 80 + i * (BUTTON_H + 30),
            w: self.width as i32 - 2 * MARGIN,
            h: BUTTON_H,
        }
    }

    pub(super) fn services_button(&self) -> Rect {
        self.tool_row(4)
    }

    pub(super) fn open_services(&mut self) -> Option<Refresh> {
        self.view = View::Services;
        Some(self.refresh(true))
    }

    pub(super) fn handle_services(&mut self, event: Event) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        if back_button(self.width).contains(x, y) {
            self.view = View::Backup;
            return Some(self.refresh(true));
        }
        if self.deepl_button().contains(x, y) {
            return self.open_deepl_form();
        }
        if self.webdav_button().contains(x, y) {
            return self.open_webdav();
        }
        if self.wallabag_button().contains(x, y) {
            return self.open_wallabag();
        }
        if self.wallabag.is_some() && self.wallabag_account_button().contains(x, y) {
            return self.open_wallabag_form();
        }
        None
    }

    pub(super) fn draw_services(&self, canvas: &mut Canvas) {
        let t = &self.text;
        draw_header(t, canvas, &tr(&Msg::Services), "", true);
        draw_button(
            t,
            canvas,
            back_button(canvas.width()),
            &tr(&Msg::Back),
            ROW_PX,
        );
        let deepl = if self.deepl_key.is_some() {
            Msg::DeeplKeySet
        } else {
            Msg::DeeplKeyButton
        };
        draw_button(t, canvas, self.deepl_button(), &tr(&deepl), ROW_PX);
        draw_button(t, canvas, self.webdav_button(), "WebDAV", ROW_PX);
        let (label, account) = if self.wallabag.is_some() {
            (Msg::WallabagSync, true)
        } else {
            (Msg::WallabagSetup, false)
        };
        draw_button(t, canvas, self.wallabag_button(), &tr(&label), ROW_PX);
        if account {
            let r = self.wallabag_account_button();
            draw_button(t, canvas, r, &tr(&Msg::WallabagAccount), ROW_PX);
        }
    }
}
