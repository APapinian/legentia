//! Yedekleme ve geri yükleme: ayarlar, kişisel kelime listesi ve her kitabın
//! okuma konumu, yer imleri, vurguları ve notları kütüphane klasörüne tek dosya olarak yazılır (USB ile
//! bilgisayara alınabilir). Geri yükleme siler değil birleştirir: eski bir yedek
//! sonradan eklenen notları kaybettirmez.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::session::BookState;
use super::settings::Settings;
use super::ui::{HEADER_BOTTOM, MARGIN, ROW_PX, Rect, back_button, draw_button, draw_header};
use super::words::{SavedWord, WORDS_FILE};
use super::{App, Back, SETTINGS_FILE, View, book_file};
use crate::canvas::Canvas;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};
use crate::storage::Store;

/// Kütüphane klasöründeki yedek dosyasının adı.
pub(super) const BACKUP_FILE: &str = "Legentia yedek.json";
const BUTTON_H: i32 = 110;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Backup {
    /// Yedeğin alındığı zaman (Unix saniyesi).
    created: u64,
    settings: Settings,
    /// Kitap kimliği → kitabın kaydı.
    books: BTreeMap<String, BookState>,
    /// Kişisel kelime listesi.
    words: Vec<SavedWord>,
}

/// Yedekteki kitap kaydını yerel kayda katar. Yer imleri ve vurgular birleşir
/// (aynısı iki tarafta varsa yereldeki kalır; yerelde notu olmayan vurguya
/// yedekteki not alınır). Okuma konumu yerelde yoksa yedekten gelir.
fn merge(local: &mut BookState, saved: BookState) {
    if local.last.is_none() {
        local.last = saved.last;
        local.percent = saved.percent;
    }
    if local.layout.is_none() {
        local.layout = saved.layout;
    }
    if local.info.is_none() {
        local.info = saved.info;
    }
    for b in saved.bookmarks {
        if !local
            .bookmarks
            .iter()
            .any(|l| l.spine == b.spine && l.position == b.position)
        {
            local.bookmarks.push(b);
        }
    }
    local.bookmarks.sort_by_key(|b| (b.spine, b.position));
    for a in saved.annotations {
        let same = local.annotations.iter_mut().find(|l| {
            l.spine == a.spine && l.start == a.start && l.end == a.end && l.quote == a.quote
        });
        match same {
            Some(l) => {
                if l.note.is_none() {
                    l.note = a.note;
                }
            }
            None => local.annotations.push(a),
        }
    }
}

impl App {
    fn backup_store(&self) -> Store {
        Store::new(&self.root)
    }

    /// Tüm kayıtları kütüphane klasörüne yazar.
    fn save_backup(&mut self) -> Msg {
        self.shutdown();
        let books: BTreeMap<String, BookState> = self
            .store
            .names("books")
            .into_iter()
            .filter_map(|id| Some((id.clone(), self.store.load(&book_file(&id))?)))
            .collect();
        let count = books.len();
        let backup = Backup {
            created: super::annotations::now(),
            settings: self.defaults.clone(),
            books,
            words: self.words.clone(),
        };
        match self.backup_store().try_save(BACKUP_FILE, &backup) {
            Ok(()) => Msg::BackupSaved { books: count },
            Err(e) => {
                log::error!("cannot write backup: {e}");
                Msg::BackupFailed
            }
        }
    }

    /// Yedeği yerel kayıtlarla birleştirir; ayarlar yedektekiyle değişir.
    fn restore_backup(&mut self) -> Msg {
        let store = self.backup_store();
        if !store.dir().join(BACKUP_FILE).exists() {
            return Msg::NoBackup;
        }
        let Some(backup) = store.load::<Backup>(BACKUP_FILE) else {
            return Msg::BackupFailed;
        };
        let count = backup.books.len();
        for (id, saved) in backup.books {
            let file = book_file(&id);
            let mut local: BookState = self.store.load(&file).unwrap_or_default();
            merge(&mut local, saved);
            self.store.save(&file, &local);
            self.progress.insert(id, local.percent);
        }
        self.defaults = backup.settings;
        self.store.save(SETTINGS_FILE, &self.defaults);
        let current = match &self.session {
            Some(s) => self
                .defaults
                .with_book_fields(s.state.layout.as_ref().unwrap_or(&self.defaults)),
            None => self.defaults.clone(),
        };
        self.use_settings(current);
        for w in backup.words {
            let k = crate::dict::key(&w.word);
            if !self.words.iter().any(|l| crate::dict::key(&l.word) == k) {
                self.words.push(w);
            }
        }
        self.words
            .sort_by(|a, b| crate::collate::compare(&a.word, &b.word));
        self.store.save(WORDS_FILE, &self.words);
        Msg::RestoreDone { books: count }
    }

    /// (Yedekle, Geri yükle) düğmeleri.
    pub(super) fn backup_buttons(&self) -> (Rect, Rect) {
        let r = |i: i32| Rect {
            x: MARGIN,
            y: HEADER_BOTTOM + 260 + i * (BUTTON_H + 40),
            w: self.width as i32 - 2 * MARGIN,
            h: BUTTON_H,
        };
        (r(0), r(1))
    }

    pub(super) fn stats_button(&self) -> Rect {
        let (_, restore) = self.backup_buttons();
        Rect {
            y: restore.y + 2 * (BUTTON_H + 40),
            ..restore
        }
    }

    pub(super) fn handle_backup(&mut self, event: Event) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        let (save, restore) = self.backup_buttons();
        let msg = if save.contains(x, y) {
            self.save_backup()
        } else if restore.contains(x, y) {
            self.restore_backup()
        } else if self.upload_button().contains(x, y) {
            return self.open_upload();
        } else if self.stats_button().contains(x, y) {
            self.view = View::Stats;
            return Some(self.refresh(true));
        } else if back_button(self.width).contains(x, y) {
            self.view = View::Library { page: 0 };
            return Some(self.refresh(true));
        } else {
            return None;
        };
        self.view = View::Message {
            msg,
            back: Back::Library,
        };
        Some(self.refresh(true))
    }

    pub(super) fn draw_backup(&self, canvas: &mut Canvas) {
        let t = &self.text;
        draw_header(t, canvas, &tr(&Msg::Tools), "", true);
        self.draw_message_lines(canvas, &Msg::BackupHint, HEADER_BOTTOM + 60);
        let (save, restore) = self.backup_buttons();
        draw_button(t, canvas, save, &tr(&Msg::BackupNow), ROW_PX);
        draw_button(t, canvas, restore, &tr(&Msg::RestoreNow), ROW_PX);
        draw_button(t, canvas, self.stats_button(), &tr(&Msg::Stats), ROW_PX);
        draw_button(t, canvas, self.upload_button(), &tr(&Msg::Upload), ROW_PX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Annotation, Bookmark, Position};

    fn ann(offset: usize, note: Option<&str>) -> Annotation {
        Annotation {
            spine: 1,
            start: Position { block: 0, offset },
            end: Position {
                block: 0,
                offset: offset + 3,
            },
            quote: "çay".into(),
            note: note.map(str::to_string),
            created: 0,
            mark: crate::app::annotations::Mark::Gray,
            tag: None,
        }
    }

    #[test]
    fn restoring_merges_without_losing_local_notes() {
        let mut local = BookState {
            last: Some((4, Position::default())),
            percent: 40,
            bookmarks: vec![],
            annotations: vec![ann(0, Some("yeni not")), ann(10, None)],
            ..BookState::default()
        };
        let saved = BookState {
            last: Some((1, Position::default())),
            percent: 10,
            bookmarks: vec![Bookmark {
                spine: 2,
                position: Position::default(),
                snippet: "x".into(),
            }],
            annotations: vec![
                ann(0, Some("eski not")),
                ann(10, Some("yedekteki")),
                ann(20, None),
            ],
            ..BookState::default()
        };
        merge(&mut local, saved);
        assert_eq!(
            local.last,
            Some((4, Position::default())),
            "yerel konum kalır"
        );
        assert_eq!(local.bookmarks.len(), 1);
        let notes: Vec<_> = local
            .annotations
            .iter()
            .map(|a| a.note.as_deref())
            .collect();
        assert_eq!(notes, vec![Some("yeni not"), Some("yedekteki"), None]);
    }
}
