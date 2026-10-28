//! Kitap içi bağlantılar: dokunulan bağlantının hedef bölümü açılır, hedef
//! kimliğin (`id`) bulunduğu sayfaya gidilir. Geri dönülecek yerler yığında
//! tutulur; yığın boş değilken sayfanın sol üstünde "← Geri" görünür.
//! Dış bağlantılar (http…) açılmaz; dokunma normal dokunma sayılır.

use super::session::{Doc, Position, Target};
use super::ui::{MARGIN, ROW_PX, Rect, draw_button};
use super::{App, View};
use crate::canvas::Canvas;
use crate::epub::resolve_href;
use crate::event::Refresh;
use crate::i18n::{Msg, tr};

/// Bağlantının kapladığı alana her iki yandan eklenen pay (piksel): dipnot
/// işaretleri tek karakterlik, parmakla isabet ettirmek zor.
const TOUCH_SLACK: f32 = 30.0;
/// Geri yığınının en fazla derinliği.
const MAX_BACK: usize = 20;

impl App {
    /// Okunan sayfada dokunulan bağlantının hedefi (`href`, olduğu gibi).
    pub(super) fn link_at(&self, x: u32, y: u32) -> Option<String> {
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
            (left - TOUCH_SLACK <= rel && rel < right + TOUCH_SLACK).then(|| link.href.clone())
        })
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
