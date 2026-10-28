//! Kitap içi bağlantılar: dokunulan bağlantının hedef bölümü açılır, hedef
//! kimliğin (`id`) bulunduğu sayfaya gidilir. Geri dönülecek yerler yığında
//! tutulur; yığın boş değilken sayfanın sol üstünde "← Geri" görünür.
//! Dış bağlantılar (http…) açılmaz; dokunma normal dokunma sayılır.
//!
//! Dipnot göndermeleri (`epub:type="noteref"` ya da "1", "[12]", "*" gibi kısa
//! bağlantı metni) sayfadan çıkmadan altta açılır pencerede gösterilir.

use super::session::{Doc, Position, Target};
use super::ui::{MARGIN, ROW_PX, Rect, draw_button};
use super::{App, View};
use crate::canvas::Canvas;
use crate::epub::{Link, resolve_href};
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};
use crate::text::{Family, Style};

/// Bağlantının kapladığı alana her iki yandan eklenen pay (piksel): dipnot
/// işaretleri tek karakterlik, parmakla isabet ettirmek zor.
const TOUCH_SLACK: f32 = 30.0;
/// Geri yığınının en fazla derinliği.
const MAX_BACK: usize = 20;
/// Bu kadar ya da daha kısa bağlantı metni dipnot işareti sayılır.
const NOTE_MARK_CHARS: usize = 5;
/// Dipnotun en fazla blok ve karakter sayısı; fazlası "Notun yerine git" ile okunur.
const NOTE_BLOCKS: usize = 6;
const NOTE_CHARS: usize = 2000;
/// Açılır pencerenin ekranın en fazla bu oranını kaplaması.
const NOTE_MAX_SHARE: f32 = 0.45;
const NOTE_PAD: i32 = 30;
const NOTE_BUTTON_H: i32 = 90;

/// Açılır penceredeki dipnot.
pub(super) struct Footnote {
    pub text: String,
    pub href: String,
}

fn looks_like_note(link: &Link, text: &str) -> bool {
    link.note || text.trim().chars().count() <= NOTE_MARK_CHARS
}
impl App {
    /// Okunan sayfada dokunulan bağlantı ve metni.
    pub(super) fn link_at(&self, x: u32, y: u32) -> Option<(Link, String)> {
        let s = self.session.as_ref()?;
        if s.pdf().is_some() {
            return None;
        }
        let page = s.pages.get(s.page)?;
        let y = y as i32;
        let line = page
            .lines
            .iter()
            .find(|l| y >= l.top && y < l.top + l.height)?;
        let block = s.blocks.get(line.block)?;
        let rel = x as f32 - self.settings.margin as f32;
        block.links.iter().find_map(|link| {
            let glyphs = line
                .line
                .glyphs
                .iter()
                .filter(|(g, _)| (link.start..link.end).contains(&g.cluster));
            let (left, right) = glyphs.fold((f32::MAX, f32::MIN), |(l, r), (g, gx)| {
                (l.min(*gx), r.max(gx + g.advance))
            });
            (left - TOUCH_SLACK <= rel && rel < right + TOUCH_SLACK).then(|| {
                let text = block.text.get(link.start..link.end).unwrap_or("");
                (link.clone(), text.to_string())
            })
        })
    }

    /// Dokunulan bağlantı: dipnotsa açılır pencere, değilse hedefine gidilir.
    pub(super) fn open_link(&mut self, link: &Link, text: &str) -> Option<Refresh> {
        if looks_like_note(link, text)
            && let Some((spine, Some(id))) = self.link_target(&link.href)
            && let Some(note) = self.note_text(spine, &id)
        {
            self.footnote = Some(Footnote {
                text: note,
                href: link.href.clone(),
            });
            self.view = View::Footnote;
            return Some(self.refresh(false));
        }
        self.follow_link(&link.href)
    }

    /// `spine` bölümünde `id` kimliğiyle başlayan notun metni: kimliğin
    /// bloğu ve ardından gelen kimliksiz bloklar (ör. `<aside>` içindeki
    /// paragraflar). Başka bölümdeki not, o bölüm dizilmeden okunur.
    fn note_text(&mut self, spine: usize, id: &str) -> Option<String> {
        let s = self.session.as_mut()?;
        let other;
        let blocks = if spine == s.spine {
            &s.blocks
        } else {
            let Doc::Epub(book) = &mut s.doc else {
                return None;
            };
            other = book.chapter(spine).ok()?;
            &other
        };
        let first = blocks
            .iter()
            .position(|b| b.anchors.iter().any(|a| a == id))?;
        let mut parts = vec![blocks[first].text.clone()];
        parts.extend(
            blocks[first + 1..]
                .iter()
                .take_while(|b| b.anchors.is_empty())
                .take(NOTE_BLOCKS - 1)
                .map(|b| b.text.clone()),
        );
        let mut text = parts.join("\n");
        if text.chars().count() > NOTE_CHARS {
            text = text.chars().take(NOTE_CHARS).collect::<String>() + "…";
        }
        Some(text)
    }

    /// Açılır pencerenin satırları ve kutusu; "Notun yerine git" düğmesi kutunun altında.
    pub(super) fn footnote_layout(&self) -> Option<(Vec<String>, Rect, Rect)> {
        let note = self.footnote.as_ref()?;
        let (family, px) = (Family::Book(self.settings.family), self.settings.font_px);
        let line_h = (px * self.settings.line_spacing) as i32;
        let max_w = (self.width as i32 - 2 * MARGIN - 2 * NOTE_PAD) as f32;
        let mut lines: Vec<String> = note
            .text
            .lines()
            .flat_map(|p| self.text.wrap_plain(p, family, Style::default(), px, max_w))
            .collect();
        let max_h = (self.height as f32 * NOTE_MAX_SHARE) as i32;
        let fits = ((max_h - 3 * NOTE_PAD - NOTE_BUTTON_H) / line_h).max(1) as usize;
        if lines.len() > fits {
            lines.truncate(fits);
            if let Some(last) = lines.last_mut() {
                last.push('…');
            }
        }
        let h = lines.len() as i32 * line_h + 3 * NOTE_PAD + NOTE_BUTTON_H;
        let panel = Rect {
            x: 0,
            y: self.height as i32 - h,
            w: self.width as i32,
            h,
        };
        let button = Rect {
            x: MARGIN,
            y: panel.y + panel.h - NOTE_PAD - NOTE_BUTTON_H,
            w: self.width as i32 - 2 * MARGIN,
            h: NOTE_BUTTON_H,
        };
        Some((lines, panel, button))
    }

    pub(super) fn handle_footnote(&mut self, event: Event) -> Option<Refresh> {
        if let Event::Tap { x, y } = event
            && let Some((_, panel, button)) = self.footnote_layout()
        {
            if button.contains(x, y) {
                let href = self.footnote.take()?.href;
                self.view = View::Reading;
                return self.follow_link(&href).or_else(|| Some(self.refresh(true)));
            }
            if panel.contains(x, y) {
                return None;
            }
        }
        // Pencerenin dışına dokunmak ya da sayfa tuşu kapatır.
        self.footnote = None;
        self.view = View::Reading;
        Some(self.refresh(false))
    }

    pub(super) fn draw_footnote(&self, canvas: &mut Canvas) {
        let Some((lines, panel, button)) = self.footnote_layout() else {
            return;
        };
        let t = &self.text;
        let (family, px) = (Family::Book(self.settings.family), self.settings.font_px);
        let line_h = (px * self.settings.line_spacing) as i32;
        canvas.fill_rect(panel.x, panel.y, panel.w as u32, panel.h as u32, 255);
        canvas.fill_rect(panel.x, panel.y, panel.w as u32, 4, 0);
        for (i, line) in lines.iter().enumerate() {
            let baseline = panel.y + NOTE_PAD + (i as i32 + 1) * line_h - (line_h - px as i32) / 2;
            t.draw_plain(
                canvas,
                line,
                family,
                Style::default(),
                MARGIN + NOTE_PAD,
                baseline,
                px,
            );
        }
        draw_button(t, canvas, button, &tr(&Msg::GoToNote), ROW_PX);
    }

    /// Bağlantının hedefi kitapta: (bölüm sırası, kimlik). Dış ya da
    /// bulunamayan hedef `None`.
    pub(super) fn link_target(&self, href: &str) -> Option<(usize, Option<String>)> {
        let s = self.session.as_ref()?;
        let Doc::Epub(book) = &s.doc else {
            return None;
        };
        if href.contains("://") || href.starts_with("mailto:") {
            return None;
        }
        let (path, fragment) = match href.split_once('#') {
            Some((p, f)) => (p, Some(f.to_string())),
            None => (href, None),
        };
        let current = book.spine.get(s.spine)?;
        let target = if path.is_empty() {
            current.clone()
        } else {
            resolve_href(current, path).ok()?
        };
        let spine = book.spine.iter().position(|p| *p == target)?;
        Some((spine, fragment.filter(|f| !f.is_empty())))
    }

    /// Bağlantıyı izler; okunan yer geri yığınına girer.
    pub(super) fn follow_link(&mut self, href: &str) -> Option<Refresh> {
        let (spine, id) = self.link_target(href)?;
        let s = self.session.as_mut()?;
        let here = (s.spine, s.position());
        if self.link_back.len() >= MAX_BACK {
            self.link_back.remove(0);
        }
        self.link_back.push(here);
        let target = id.map_or(Target::Start, Target::Anchor);
        self.open_chapter(spine, target);
        Some(self.refresh(true))
    }

    /// "← Geri": bağlantıdan önceki yere döner.
    pub(super) fn link_back_button(&self) -> Option<Rect> {
        (!self.link_back.is_empty() && matches!(self.view, View::Reading)).then_some(Rect {
            x: MARGIN,
            y: 10,
            w: 200,
            h: 70,
        })
    }

    pub(super) fn go_back(&mut self) -> Option<Refresh> {
        let (spine, at): (usize, Position) = self.link_back.pop()?;
        self.open_chapter(spine, Target::At(at));
        Some(self.refresh(true))
    }

    pub(super) fn draw_link_back(&self, canvas: &mut Canvas) {
        if let Some(r) = self.link_back_button() {
            draw_button(&self.text, canvas, r, &tr(&Msg::LinkBack), ROW_PX);
        }
    }
}
