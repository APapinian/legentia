//! İndirme kuyruğu: katalogdan seçilen kitaplar sırayla, arka planda
//! `<kütüphane>/İndirilenler` klasörüne "Yazar - Başlık.epub" adıyla iner.
//! Bu sırada gezinmeye devam edilir; ilerleme "İndirmeler" ekranında ve
//! kitabın ayrıntısında görünür. Aynı dosya zaten varsa yeniden indirilmez,
//! "Aç" düğmesi çıkar. Kuyruk yalnızca uygulama açıkken tutulur.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::catalog::auth_for;
use super::jobs::Job;
use super::ui::{
    BOLD, HEADER_BOTTOM, List, MARGIN, PLAIN, ROW_PX, Rect, back_button, draw_button, draw_footer,
    draw_header, left_of, one_line,
};
use super::{App, Back, View};
use crate::canvas::Canvas;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};
use crate::library::{self, Format};
use crate::net::{Auth, Net, NetError};
use crate::opds::EntryKind;
use crate::storage::free_space;
use crate::text::Family;

pub(super) const DOWNLOAD_DIR: &str = "İndirilenler";
/// Tek kitabın en büyük boyutu.
const DOWNLOAD_LIMIT: u64 = 300 << 20;
/// İndirme diskte en az bu kadar boş yer bırakır: ayarlar ve notlar yazılabilsin.
const RESERVE: u64 = 20 << 20;
/// Boş alan en geç bu kadar bayt indirmede bir yeniden ölçülür.
const SPACE_CHECK_EVERY: u64 = 1 << 20;
/// Dosya adının en fazla karakter sayısı (FAT32 255 sınırının epey altında).
const NAME_CHARS: usize = 120;
pub(super) const DOWNLOADS: List = List {
    top: HEADER_BOTTOM,
    row_h: 120,
};
const ROW_BUTTON_W: i32 = 240;
const DOWNLOADS_BUTTON_W: i32 = 290;

#[derive(Default)]
pub(super) struct Progress {
    done: AtomicU64,
    /// 0: sunucu boyutu bildirmedi.
    total: AtomicU64,
}

impl Progress {
    fn get(&self) -> (u64, Option<u64>) {
        let total = self.total.load(Ordering::Relaxed);
        (
            self.done.load(Ordering::Relaxed),
            (total > 0).then_some(total),
        )
    }
}

pub(super) enum State {
    Queued,
    Running {
        job: Job<Result<u64, Msg>>,
        progress: Arc<Progress>,
    },
    Failed(Msg),
    Done,
}

pub(super) struct Download {
    pub title: String,
    url: String,
    auth: Option<Auth>,
    pub dest: PathBuf,
    pub state: State,
    /// Ekranda son gösterilen onluk dilim (%0, %10…); E Ink her baytta yenilenmez.
    shown: u32,
}

impl Download {
    fn active(&self) -> bool {
        matches!(self.state, State::Queued | State::Running { .. })
    }

    fn percent(&self) -> Option<u32> {
        let State::Running { progress, .. } = &self.state else {
            return None;
        };
        let (done, total) = progress.get();
        total.map(|t| (done * 100 / t).min(100) as u32)
    }

    /// Satırdaki ya da ayrıntıdaki düğme.
    fn action(&self) -> Action {
        match self.state {
            State::Queued | State::Running { .. } => Action::Cancel,
            State::Failed(_) => Action::Retry,
            State::Done => Action::Open,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Download,
    Cancel,
    Retry,
    Open,
}

/// "Yazar - Başlık.epub"; dosya sistemine (FAT32) uymayan karakterler boşluk
/// olur, baştaki nokta (gizli dosya) ve sondaki nokta/boşluk atılır.
pub(super) fn file_name(authors: &[String], title: &str, format: Format) -> String {
    let base = match authors.first().map(|a| a.trim()) {
        Some(a) if !a.is_empty() => format!("{a} - {title}"),
        _ => title.to_string(),
    };
    let clean: String = base
        .chars()
        .map(|c| {
            if c.is_control() || r#"/\:*?"<>|"#.contains(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let clean: String = clean
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(NAME_CHARS)
        .collect();
    let mut clean = clean
        .trim_start_matches(['.', ' '])
        .trim_end_matches(['.', ' '])
        .to_string();
    if clean.is_empty() {
        clean = tr(&Msg::UntitledBook);
    }
    let ext = match format {
        Format::Epub => "epub",
        Format::Pdf => "pdf",
    };
    format!("{clean}.{ext}")
}

/// Boş alan kalan `remaining` baytı ve `RESERVE`'ü karşılıyor mu; ölçülemiyorsa evet.
fn has_room(free: Option<u64>, remaining: u64) -> bool {
    free.is_none_or(|f| f >= remaining.saturating_add(RESERVE))
}

/// Ayrı iş parçacığında çalışır.
fn run(
    net: &Net,
    url: &str,
    auth: Option<&Auth>,
    dest: &Path,
    progress: &Progress,
    cancel: &AtomicBool,
) -> Result<u64, Msg> {
    let dir = dest.parent().unwrap_or(Path::new("."));
    if let Err(e) = std::fs::create_dir_all(dir) {
        log::error!("cannot create {}: {e}", dir.display());
        return Err(Msg::NetDisk);
    }
    let mut no_space = false;
    let mut checked_at: Option<u64> = None;
    let result = net.download(url, auth, dest, DOWNLOAD_LIMIT, |done, total| {
        progress.done.store(done, Ordering::Relaxed);
        progress.total.store(total.unwrap_or(0), Ordering::Relaxed);
        if checked_at.is_none_or(|at| done - at >= SPACE_CHECK_EVERY) {
            checked_at = Some(done);
            let remaining = total.map_or(0, |t| t.saturating_sub(done));
            if !has_room(free_space(dir), remaining) {
                no_space = true;
                return false;
            }
        }
        !cancel.load(Ordering::Relaxed)
    });
    match result {
        Ok(n) => {
            log::info!("downloaded {} ({n} bytes)", dest.display());
            Ok(n)
        }
        Err(NetError::Cancelled) if no_space => Err(Msg::NoSpace),
        Err(e) => {
            log::warn!("cannot download {url}: {e:?}");
            Err(e.message())
        }
    }
}

impl App {
    /// Arka plan işi sürüyor: platform `Event::Tick` göndermeli.
    pub fn wants_ticks(&self) -> bool {
        self.browser.job.is_some()
            || self.downloads.iter().any(Download::active)
            || self.online.is_some()
    }

    fn download_of(&self, dest: &Path) -> Option<usize> {
        self.downloads.iter().position(|d| d.dest == dest)
    }

    /// Katalog sayfasındaki `index`. kitabın indirme bilgisi: (başlık, adres, yetki, hedef).
    fn entry_download(&self, index: usize) -> Option<(String, String, Option<Auth>, PathBuf)> {
        let e = self.browser.feeds.last()?.feed.entries.get(index)?;
        let EntryKind::Book(links) = &e.kind else {
            return None;
        };
        let (format, url) = links.first()?;
        let catalog = self.browser.catalogs.get(self.browser.current)?;
        let dest = self
            .root
            .join(DOWNLOAD_DIR)
            .join(file_name(&e.authors, &e.title, *format));
        Some((
            e.title.clone(),
            url.clone(),
            auth_for(catalog, url).cloned(),
            dest,
        ))
    }

    /// Ayrıntı ekranındaki düğme ve etiketi; indirme hata verdiyse nedeni.
    pub(super) fn entry_action(&self, index: usize) -> Option<(Action, Msg, Option<Msg>)> {
        let (.., dest) = self.entry_download(index)?;
        Some(match self.download_of(&dest).map(|i| &self.downloads[i]) {
            Some(
                d @ Download {
                    state: State::Running { .. },
                    ..
                },
            ) => (Action::Cancel, Msg::CancelDownloading(d.percent()), None),
            Some(Download {
                state: State::Queued,
                ..
            }) => (Action::Cancel, Msg::CancelQueued, None),
            Some(Download {
                state: State::Failed(m),
                ..
            }) => (Action::Retry, Msg::Retry, Some(m.clone())),
            _ if dest.exists() => (Action::Open, Msg::OpenBook, None),
            _ => (Action::Download, Msg::Download, None),
        })
    }

    /// Ayrıntı ekranındaki düğmeye dokunuldu.
    pub(super) fn entry_pressed(&mut self, index: usize) -> Option<Refresh> {
        let (action, ..) = self.entry_action(index)?;
        let (title, url, auth, dest) = self.entry_download(index)?;
        if action == Action::Download {
            self.downloads.push(Download {
                title,
                url,
                auth,
                dest,
                state: State::Queued,
                shown: 0,
            });
            self.start_next();
            return Some(self.refresh(false));
        }
        match self.download_of(&dest) {
            Some(i) => self.download_action(i, action),
            None => {
                self.open_book(&dest);
                Some(self.refresh(true))
            }
        }
    }

    /// Sürmekte olan indirme yoksa sıradakini başlatır.
    fn start_next(&mut self) {
        if self
            .downloads
            .iter()
            .any(|d| matches!(d.state, State::Running { .. }))
        {
            return;
        }
        let net = self.browser.net();
        let Some(d) = self
            .downloads
            .iter_mut()
            .find(|d| matches!(d.state, State::Queued))
        else {
            return;
        };
        let progress = Arc::new(Progress::default());
        let (url, auth, dest, p) = (
            d.url.clone(),
            d.auth.clone(),
            d.dest.clone(),
            progress.clone(),
        );
        let job = Job::spawn(move |cancel| run(&net, &url, auth.as_ref(), &dest, &p, cancel));
        d.state = State::Running { job, progress };
        d.shown = 0;
    }

    /// `Event::Tick`: indirmelerin durumu. Ekran yalnızca gösterdiği şey
    /// değiştiyse yenilenir (ilerleme %10 adımlarla).
    pub(super) fn poll_downloads(&mut self) -> Option<Refresh> {
        let (mut changed, mut progressed, mut finished) = (false, false, false);
        for d in &mut self.downloads {
            let State::Running { job, .. } = &d.state else {
                continue;
            };
            match job.poll() {
                None => {
                    let step = d.percent().unwrap_or(0) / 10;
                    if step != d.shown {
                        d.shown = step;
                        progressed = true;
                    }
                }
                Some(result) => {
                    d.state = match result {
                        Some(Ok(_)) => {
                            finished = true;
                            State::Done
                        }
                        Some(Err(msg)) => State::Failed(msg),
                        None => State::Failed(Msg::NetInvalid),
                    };
                    changed = true;
                }
            }
        }
        if finished {
            self.library = library::scan(&self.root, &self.store);
        }
        if changed {
            self.start_next();
        }
        let visible = match self.view {
            View::Downloads { .. } | View::Entry { .. } => changed || progressed,
            View::Catalogs { .. } | View::Feed => changed,
            View::Library { .. } => finished,
            _ => false,
        };
        visible.then(|| self.refresh(false))
    }

    fn download_action(&mut self, i: usize, action: Action) -> Option<Refresh> {
        match action {
            Action::Cancel => {
                if let State::Running { job, .. } = &self.downloads[i].state {
                    // İş parçacığı bir sonraki parçada durur ve yarım dosyayı siler.
                    job.cancel();
                }
                self.downloads.remove(i);
                self.start_next();
            }
            Action::Retry => {
                self.downloads[i].state = State::Queued;
                self.start_next();
            }
            Action::Open => {
                let dest = self.downloads[i].dest.clone();
                self.open_book(&dest);
                return Some(self.refresh(true));
            }
            Action::Download => return None,
        }
        Some(self.refresh(false))
    }

    /// Başlıkta `right` düğmesinin solundaki "İndirmeler" düğmesi (kuyruk boş değilse).
    pub(super) fn downloads_button(&self, right: Rect) -> Option<Rect> {
        (!self.downloads.is_empty()).then(|| left_of(right, DOWNLOADS_BUTTON_W))
    }

    pub(super) fn draw_downloads_button(&self, canvas: &mut Canvas, r: Rect) {
        let active = self.downloads.iter().filter(|d| d.active()).count();
        draw_button(
            &self.text,
            canvas,
            r,
            &tr(&Msg::DownloadsButton(active)),
            ROW_PX,
        );
    }

    pub(super) fn row_button(&self, row: usize) -> Rect {
        let r = DOWNLOADS.row_rect(self.width, row);
        Rect {
            x: self.width as i32 - MARGIN - ROW_BUTTON_W,
            y: r.y + 20,
            w: ROW_BUTTON_W,
            h: 80,
        }
    }

    pub(super) fn handle_downloads(
        &mut self,
        page: usize,
        back: Back,
        event: Event,
    ) -> Option<Refresh> {
        let len = self.downloads.len();
        if let Event::Tap { x, y } = event {
            if back_button(self.width).contains(x, y) {
                self.view = match back {
                    Back::Feed => View::Feed,
                    _ => View::Catalogs { page: 0 },
                };
                return Some(self.refresh(true));
            }
            let i = DOWNLOADS.row_at(page, len, y)?;
            if !self
                .row_button(i - page * DOWNLOADS.per_page())
                .contains(x, y)
            {
                return None;
            }
            let action = self.downloads[i].action();
            let refresh = self.download_action(i, action);
            if let View::Downloads { page, .. } = &mut self.view {
                *page = (*page).min(DOWNLOADS.pages(self.downloads.len()) - 1);
            }
            return refresh;
        }
        let page = Self::page_list(page, DOWNLOADS.pages(len), event)?;
        self.view = View::Downloads { page, back };
        Some(self.refresh(false))
    }

    pub(super) fn draw_downloads(&self, canvas: &mut Canvas, page: usize) {
        let t = &self.text;
        draw_header(
            t,
            canvas,
            &tr(&Msg::Downloads),
            &tr(&Msg::DownloadCount(self.downloads.len())),
            true,
        );
        if self.downloads.is_empty() {
            self.draw_message_lines(canvas, &Msg::NoDownloads, HEADER_BOTTOM + 80);
        }
        let max_w = (canvas.width() as i32 - 3 * MARGIN - ROW_BUTTON_W) as f32;
        for (i, row) in DOWNLOADS.visible(page, self.downloads.len()) {
            let d = &self.downloads[i];
            let r = DOWNLOADS.row_rect(canvas.width(), row);
            let title = one_line(t, &d.title, BOLD, ROW_PX, max_w);
            t.draw_plain(canvas, &title, Family::Ui, BOLD, MARGIN, r.y + 50, ROW_PX);
            let status = match &d.state {
                State::Queued => Msg::Queued,
                State::Running { progress, .. } => {
                    let (done, total) = progress.get();
                    Msg::Progress { done, total }
                }
                State::Failed(m) => m.clone(),
                State::Done => Msg::Downloaded,
            };
            let status = one_line(t, &tr(&status), PLAIN, 30.0, max_w);
            t.draw_plain(canvas, &status, Family::Ui, PLAIN, MARGIN, r.y + 95, 30.0);
            let label = match d.action() {
                Action::Cancel => Msg::CancelDownload,
                Action::Retry => Msg::Retry,
                Action::Open | Action::Download => Msg::OpenBook,
            };
            draw_button(t, canvas, self.row_button(row), &tr(&label), 32.0);
        }
        let footer = Msg::ListFooter {
            page: page + 1,
            pages: DOWNLOADS.pages(self.downloads.len()),
        };
        draw_footer(t, canvas, &footer, MARGIN);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_readable_and_safe() {
        let authors = vec!["Dostoyevsky, Fyodor".to_string()];
        assert_eq!(
            file_name(&authors, "Crime and Punishment", Format::Epub),
            "Dostoyevsky, Fyodor - Crime and Punishment.epub"
        );
        assert_eq!(
            file_name(&[], "Soru: neden? / \"Cevap\"\n", Format::Pdf),
            "Soru neden Cevap.pdf"
        );
        assert_eq!(file_name(&[], "...gizli...", Format::Epub), "gizli.epub");
        assert_eq!(
            file_name(&[" ".into()], "", Format::Epub),
            "Adsız kitap.epub"
        );
        let long = "ç".repeat(500);
        assert_eq!(
            file_name(&[], &long, Format::Epub).chars().count(),
            NAME_CHARS + 5
        );
    }

    #[test]
    fn room_keeps_a_reserve() {
        assert!(has_room(None, u64::MAX), "ölçülemiyorsa engellenmez");
        assert!(has_room(Some(RESERVE + 10), 10));
        assert!(!has_room(Some(RESERVE + 9), 10));
        assert!(!has_room(Some(5), u64::MAX));
        assert!(free_space(&std::env::temp_dir()).is_some_and(|f| f > 0));
    }
}
