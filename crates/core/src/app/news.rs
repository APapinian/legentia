//! Haberler (RSS/Atom): Araçlar → Haberler. Beslemeler ve indirilen yazılar
//! veri klasöründe `haberler.json`'da tutulur (çevrimdışı okunur); okunan
//! yazılar işaretlenir. Yazılar kitaba çevrilmez, ayrı ekranda okunur:
//! besleme listesi → yazı listesi → yazı (okuma ayarlarıyla sayfalanır).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::form::{Field, Form, FormKind};
use super::jobs::Job;
use super::session::{Page, paginate};
use super::ui::{
    BOLD, HEADER_BOTTOM, List, MARGIN, PLAIN, ROW_PX, back_button, draw_button, draw_footer,
    draw_header_until, header_button, left_of, one_line,
};
use super::{App, View};
use crate::canvas::Canvas;
use crate::event::{Event, PageKey, Refresh};
use crate::i18n::{Msg, tr};
use crate::net::Net;
use crate::rss::Item;
use crate::text::Family;

pub(super) const NEWS_FILE: &str = "haberler.json";
/// Besleme başına saklanan en çok yazı.
const MAX_ITEMS: usize = 100;
const FEED_LIMIT: u64 = 8 << 20;
pub(super) const NEWS_LIST: List = List {
    top: HEADER_BOTTOM,
    row_h: 120,
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Feed {
    pub url: String,
    pub title: String,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct News {
    pub feeds: Vec<Feed>,
    /// Okunan yazıların kimlikleri.
    pub read: BTreeSet<String>,
}

/// Güncellemenin sonucu: besleme adresi → okunan besleme ya da hata.
type Fetched = Vec<(String, Result<crate::rss::Channel, Msg>)>;

/// Haberler durumu: kayıt, güncelleme işi, açık yazının sayfaları.
#[derive(Default)]
pub(super) struct NewsState {
    pub news: News,
    pub job: Option<Job<Fetched>>,
    /// Son güncellemede ulaşılamayan beslemeler.
    pub failed: Vec<String>,
    pub article: Vec<Page>,
}

impl News {
    fn unread(&self, feed: &Feed) -> usize {
        feed.items
            .iter()
            .filter(|i| !self.read.contains(&i.id))
            .count()
    }
}

fn fetch(net: &Net, url: &str) -> Result<crate::rss::Channel, Msg> {
    let body = net.get(url, None, FEED_LIMIT).map_err(|e| {
        log::warn!("feed {url}: {e:?}");
        e.message()
    })?;
    crate::rss::parse(&String::from_utf8_lossy(&body)).ok_or(Msg::NotAFeed)
}

impl App {
    pub(super) fn news_button(&self) -> super::ui::Rect {
        self.tool_row(6)
    }

    fn add_feed_button(&self) -> super::ui::Rect {
        header_button(self.width, 170)
    }

    fn refresh_feeds_button(&self) -> super::ui::Rect {
        left_of(self.add_feed_button(), 220)
    }

    fn save_news(&self) {
        self.store.save(NEWS_FILE, &self.news.news);
    }

    pub(super) fn open_news(&mut self) -> Option<Refresh> {
        self.view = View::NewsFeeds { page: 0 };
        Some(self.refresh(true))
    }

    /// Bütün beslemeleri arka planda indirir.
    fn update_news(&mut self) -> Option<Refresh> {
        if !self.settings.network {
            self.show_message(Msg::NetworkOff);
            return Some(self.refresh(true));
        }
        let urls: Vec<String> = self.news.news.feeds.iter().map(|f| f.url.clone()).collect();
        if urls.is_empty() || self.news.job.is_some() {
            return None;
        }
        let net = self.browser.net();
        self.news.job = Some(Job::spawn(move |cancel| {
            let mut out = Vec::new();
            for url in urls {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                let r = fetch(&net, &url);
                out.push((url, r));
            }
            out
        }));
        Some(self.refresh(false))
    }

    /// `Event::Tick`: güncelleme bittiyse yazıları birleştirir.
    pub(super) fn poll_news(&mut self) -> Option<Refresh> {
        let done = self.news.job.as_ref()?.poll()?;
        self.news.job = None;
        self.news.failed.clear();
        for (url, result) in done.unwrap_or_default() {
            let Some(feed) = self.news.news.feeds.iter_mut().find(|f| f.url == url) else {
                continue;
            };
            match result {
                Ok(channel) => {
                    if !channel.title.is_empty() {
                        feed.title = channel.title;
                    }
                    feed.items = channel.items.into_iter().take(MAX_ITEMS).collect();
                }
                Err(_) => self.news.failed.push(url),
            }
        }
        // Artık beslemede olmayan yazıların okundu bilgisi atılır.
        let News { feeds, read } = &mut self.news.news;
        read.retain(|id| feeds.iter().any(|f| f.items.iter().any(|i| &i.id == id)));
        self.save_news();
        matches!(self.view, View::NewsFeeds { .. } | View::NewsItems { .. })
            .then(|| self.refresh(false))
    }

    pub(super) fn open_feed_form(&mut self, edit: Option<usize>) -> Option<Refresh> {
        let url = edit
            .and_then(|i| self.news.news.feeds.get(i))
            .map(|f| f.url.clone())
            .unwrap_or_default();
        let kind = match edit {
            Some(i) => FormKind::EditFeed(i),
            None => FormKind::NewFeed,
        };
        self.open_form(Form::new(kind, vec![Field::new(Msg::FieldFeedUrl, &url)]))
    }

    /// Formdaki adresi ekler ya da değiştirir; geçersizse hata.
    pub(super) fn save_feed(&mut self, edit: Option<usize>, url: &str) -> Result<(), Msg> {
        let url = url.trim();
        let url = if url.contains("://") {
            url.to_string()
        } else {
            format!("https://{url}")
        };
        if url.contains(' ') || !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(Msg::InvalidUrl);
        }
        match edit.and_then(|i| self.news.news.feeds.get_mut(i)) {
            Some(f) if f.url != url => {
                f.url = url;
                f.items.clear();
            }
            Some(_) => {}
            None => self.news.news.feeds.push(Feed {
                title: url.clone(),
                url,
                items: Vec::new(),
            }),
        }
        self.save_news();
        Ok(())
    }

    pub(super) fn delete_feed(&mut self, i: usize) {
        if i < self.news.news.feeds.len() {
            self.news.news.feeds.remove(i);
            self.save_news();
        }
    }

    pub(super) fn handle_news_feeds(&mut self, page: usize, event: Event) -> Option<Refresh> {
        let feeds = self.news.news.feeds.len();
        match event {
            Event::Tap { x, y } => {
                if back_button(self.width).contains(x, y) {
                    self.view = View::Backup;
                    return Some(self.refresh(true));
                }
                if self.add_feed_button().contains(x, y) {
                    return self.open_feed_form(None);
                }
                if self.refresh_feeds_button().contains(x, y) {
                    return self.update_news();
                }
                let feed = NEWS_LIST.row_at(page, feeds, y)?;
                self.view = View::NewsItems { feed, page: 0 };
                Some(self.refresh(true))
            }
            Event::Hold { y, .. } => {
                let feed = NEWS_LIST.row_at(page, feeds, y)?;
                self.open_feed_form(Some(feed))
            }
            _ => {
                let page = Self::page_list(page, NEWS_LIST.pages(feeds), event)?;
                self.view = View::NewsFeeds { page };
                Some(self.refresh(false))
            }
        }
    }

    pub(super) fn draw_news_feeds(&self, canvas: &mut Canvas, page: usize) {
        let t = &self.text;
        let n = &self.news.news;
        let unread: usize = n.feeds.iter().map(|f| n.unread(f)).sum();
        let update = self.refresh_feeds_button();
        draw_header_until(
            t,
            canvas,
            &tr(&Msg::News),
            &tr(&Msg::UnreadCount(unread)),
            update.x - 30,
        );
        draw_button(
            t,
            canvas,
            back_button(canvas.width()),
            &tr(&Msg::Back),
            ROW_PX,
        );
        draw_button(
            t,
            canvas,
            self.add_feed_button(),
            &tr(&Msg::AddCatalog),
            ROW_PX,
        );
        let label = if self.news.job.is_some() {
            Msg::Loading
        } else {
            Msg::UpdateNews
        };
        draw_button(t, canvas, update, &tr(&label), ROW_PX);
        if n.feeds.is_empty() {
            self.draw_message_lines(canvas, &Msg::NoFeeds, HEADER_BOTTOM + 80);
        }
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
        for (index, row) in NEWS_LIST.visible(page, n.feeds.len()) {
            let f = &n.feeds[index];
            let r = NEWS_LIST.row_rect(canvas.width(), row);
            let title = one_line(t, &f.title, BOLD, ROW_PX, max_w);
            t.draw_plain(canvas, &title, Family::Ui, BOLD, MARGIN, r.y + 50, ROW_PX);
            let second = if self.news.failed.contains(&f.url) {
                Msg::FeedFailed
            } else {
                Msg::FeedLine {
                    unread: n.unread(f),
                    total: f.items.len(),
                }
            };
            let second = one_line(t, &tr(&second), PLAIN, 30.0, max_w);
            t.draw_plain(canvas, &second, Family::Ui, PLAIN, MARGIN, r.y + 95, 30.0);
        }
        let footer = Msg::ListFooter {
            page: page + 1,
            pages: NEWS_LIST.pages(n.feeds.len()),
        };
        draw_footer(t, canvas, &footer, MARGIN);
    }

    fn all_read_button(&self) -> super::ui::Rect {
        header_button(self.width, 280)
    }

    pub(super) fn handle_news_items(
        &mut self,
        feed: usize,
        page: usize,
        event: Event,
    ) -> Option<Refresh> {
        let len = self.news.news.feeds.get(feed)?.items.len();
        if let Event::Tap { x, y } = event {
            if back_button(self.width).contains(x, y) {
                self.view = View::NewsFeeds { page: 0 };
                return Some(self.refresh(true));
            }
            if self.all_read_button().contains(x, y) {
                let ids: Vec<String> = self.news.news.feeds[feed]
                    .items
                    .iter()
                    .map(|i| i.id.clone())
                    .collect();
                self.news.news.read.extend(ids);
                self.save_news();
                return Some(self.refresh(false));
            }
            let item = NEWS_LIST.row_at(page, len, y)?;
            return self.open_article(feed, item);
        }
        let page = Self::page_list(page, NEWS_LIST.pages(len), event)?;
        self.view = View::NewsItems { feed, page };
        Some(self.refresh(false))
    }

    pub(super) fn draw_news_items(&self, canvas: &mut Canvas, feed: usize, page: usize) {
        let t = &self.text;
        let n = &self.news.news;
        let Some(f) = n.feeds.get(feed) else {
            return;
        };
        let all = self.all_read_button();
        draw_header_until(
            t,
            canvas,
            &f.title,
            &tr(&Msg::UnreadCount(n.unread(f))),
            all.x - 30,
        );
        draw_button(
            t,
            canvas,
            back_button(canvas.width()),
            &tr(&Msg::Back),
            ROW_PX,
        );
        draw_button(t, canvas, all, &tr(&Msg::MarkAllRead), ROW_PX);
        if f.items.is_empty() {
            self.draw_message_lines(canvas, &Msg::NoItems, HEADER_BOTTOM + 80);
        }
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
        for (index, row) in NEWS_LIST.visible(page, f.items.len()) {
            let item = &f.items[index];
            let r = NEWS_LIST.row_rect(canvas.width(), row);
            // Okunmamış yazı kalın.
            let style = if n.read.contains(&item.id) {
                PLAIN
            } else {
                BOLD
            };
            let title = one_line(t, &item.title, style, ROW_PX, max_w);
            t.draw_plain(canvas, &title, Family::Ui, style, MARGIN, r.y + 50, ROW_PX);
            let date = one_line(t, &item.date, PLAIN, 28.0, max_w);
            t.draw_plain(canvas, &date, Family::Ui, PLAIN, MARGIN, r.y + 95, 28.0);
        }
        let footer = Msg::ListFooter {
            page: page + 1,
            pages: NEWS_LIST.pages(f.items.len()),
        };
        draw_footer(t, canvas, &footer, MARGIN);
    }

    /// Yazıyı okuma ayarlarıyla sayfalayıp açar ve okundu işaretler.
    fn open_article(&mut self, feed: usize, item: usize) -> Option<Refresh> {
        let it = self.news.news.feeds.get(feed)?.items.get(item)?.clone();
        let mut blocks = crate::epub::html_blocks(&format!("<h2>{}</h2>", escape(&it.title)));
        blocks.extend(crate::epub::html_blocks(&it.html));
        if !it.link.is_empty() {
            blocks.extend(crate::epub::html_blocks(&format!(
                "<p><i>{}</i></p>",
                escape(&it.link)
            )));
        }
        self.news.article = paginate(
            &self.text,
            &self.settings,
            None,
            (self.width, self.height),
            &blocks,
        );
        self.news.news.read.insert(it.id);
        self.save_news();
        self.view = View::NewsArticle {
            feed,
            item,
            page: 0,
        };
        Some(self.refresh(true))
    }

    pub(super) fn handle_news_article(
        &mut self,
        feed: usize,
        item: usize,
        page: usize,
        event: Event,
    ) -> Option<Refresh> {
        let pages = self.news.article.len();
        let third = self.width / 3;
        let next = match event {
            Event::Key(PageKey::Forward) => page + 1,
            Event::Key(PageKey::Back) => page.wrapping_sub(1),
            Event::Tap { x, .. } if x < third => page.wrapping_sub(1),
            Event::Tap { x, .. } if x > 2 * third => page + 1,
            Event::Tap { .. } => {
                // Ortaya dokunma: yazı listesine dönüş.
                self.news.article.clear();
                self.view = View::NewsItems {
                    feed,
                    page: item / NEWS_LIST.per_page(),
                };
                return Some(self.refresh(true));
            }
            _ => return None,
        };
        if next >= pages {
            return None;
        }
        self.view = View::NewsArticle {
            feed,
            item,
            page: next,
        };
        Some(self.refresh(false))
    }

    pub(super) fn draw_news_article(&self, canvas: &mut Canvas, page: usize) {
        if let Some(p) = self.news.article.get(page) {
            for placed in &p.lines {
                self.text
                    .draw_line(canvas, &placed.line, self.settings.margin, placed.baseline);
            }
        }
        let footer = Msg::ArticleFooter {
            page: page + 1,
            pages: self.news.article.len(),
        };
        draw_footer(&self.text, canvas, &footer, MARGIN);
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
