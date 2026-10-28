//! Sözlükte arama ekranı: seçilen kelime (ya da kökü) bütün sözlüklerde
//! aranır, maddeler okuyucunun dizgisiyle tam ekran gösterilir.

use std::path::PathBuf;

use super::session::{Page, paginate};
use super::ui::{MARGIN, draw_footer};
use super::{App, Back, View};
use crate::canvas::Canvas;
use crate::dict::{Dictionaries, render};
use crate::epub::{Block, BlockKind};
use crate::event::{Event, PageKey, Refresh};
use crate::i18n::{Msg, tr};
use crate::text::{Span, Style};

/// Kütüphane köküne göre sözlük klasörleri: KOReader'ın sözlükleri ve bizimki.
const DICT_DIRS: [&str; 2] = [".adds/koreader/data/dict", "Sözlükler"];

/// Gösterilen sözlük sonucu, sayfalara dizilmiş.
pub(super) struct Lookup {
    pub pages: Vec<Page>,
}

impl App {
    /// Sözlükler ilk aramada açılır (dizin kurmak büyük sözlükte zaman alır).
    fn dictionaries(&mut self) -> &Dictionaries {
        self.dicts.get_or_insert_with(|| {
            let dirs: Vec<PathBuf> = DICT_DIRS.iter().map(|d| self.root.join(d)).collect();
            let started = std::time::Instant::now();
            let (dicts, failed) = Dictionaries::open(&dirs);
            for (path, e) in failed {
                log::warn!("cannot open dictionary {}: {e:?}", path.display());
            }
            log::info!(
                "dictionaries {:?} opened in {:?}",
                dicts.names(),
                started.elapsed()
            );
            dicts
        })
    }

    /// `word`'ü arar; bulunursa tanım ekranını, bulunamazsa açıklamayı gösterir.
    pub(super) fn look_up(&mut self, word: &str) -> Option<Refresh> {
        let width = self.width;
        if self.dictionaries().is_empty() {
            self.show_message(Msg::NoDictionaries);
            return Some(self.refresh(true));
        }
        let Some(def) = self.dictionaries().define(word) else {
            self.show_message(Msg::NotInDictionary(word.trim().to_string()));
            return Some(self.refresh(true));
        };
        let mut blocks = Vec::new();
        let asked = crate::dict::key(word);
        if crate::dict::key(&def.form) != asked {
            blocks.push(italic(&tr(&Msg::LookupForm {
                word: word.trim().to_string(),
                form: def.form.clone(),
            })));
        }
        for (dict, article) in &def.articles {
            blocks.extend(render::article_blocks(dict, article));
        }
        let pages = paginate(&self.text, &self.settings, Some("tr"), width, &blocks);
        self.lookup = Some(Lookup { pages });
        self.view = View::Lookup { page: 0 };
        Some(self.refresh(true))
    }

    fn show_message(&mut self, msg: Msg) {
        self.view = View::Message {
            msg,
            back: Back::Reading,
        };
    }

    pub(super) fn handle_lookup(&mut self, page: usize, event: Event) -> Option<Refresh> {
        let pages = self.lookup.as_ref().map_or(0, |l| l.pages.len());
        let third = self.width / 3;
        let next = match event {
            Event::Key(PageKey::Forward) => page + 1,
            Event::Key(PageKey::Back) => page.wrapping_sub(1),
            Event::Tap { x, .. } if x < third => page.wrapping_sub(1),
            Event::Tap { x, .. } if x > 2 * third => page + 1,
            Event::Tap { .. } => {
                self.lookup = None;
                self.view = View::Reading;
                return Some(self.refresh(true));
            }
            _ => return None,
        };
        if next >= pages {
            return None;
        }
        self.view = View::Lookup { page: next };
        Some(self.refresh(false))
    }

    pub(super) fn draw_lookup(&self, canvas: &mut Canvas, page: usize) {
        let Some(l) = &self.lookup else {
            return;
        };
        if let Some(p) = l.pages.get(page) {
            for placed in &p.lines {
                self.text
                    .draw_line(canvas, &placed.line, self.settings.margin, placed.baseline);
            }
        }
        let footer = Msg::LookupFooter {
            page: page + 1,
            pages: l.pages.len(),
        };
        draw_footer(&self.text, canvas, &footer, MARGIN);
    }
}

fn italic(text: &str) -> Block {
    Block {
        kind: BlockKind::Paragraph,
        text: text.to_string(),
        spans: vec![Span {
            start: 0,
            end: text.len(),
            style: Style {
                italic: true,
                ..Style::default()
            },
        }],
    }
}
