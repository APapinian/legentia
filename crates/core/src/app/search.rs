//! Kitap içinde arama: bütün bölümler (PDF'te sayfalar) taranır, sonuçlar
//! bölüm adı ve kelimenin çevresiyle listelenir. Büyük/küçük harf ve şapka
//! fark etmez ("kağıt" "kâğıt"ı bulur). Noktalı ve noktasız i (ı, i, I, İ)
//! aynı sayılır: kitabın dili bilinmeyebilir (PDF) ve İngilizce "Index"
//! Türkçe kuralla "ındex" olurdu; aramada fazla sonuç, kaçan sonuçtan iyidir.
//! "ğ" ile "g", "ş" ile "s" ayrı harftir. Sonuca dokununca o sayfa açılır, bulunan yer
//! işaretlenir; sol üstteki "← Sonuçlar" listeye döndürür.

use super::screens::SEARCH;
use super::select::box_areas;
use super::session::{Doc, Position, Target};
use super::ui::{
    BOLD, HEADER_BOTTOM, MARGIN, PLAIN, ROW_PX, Rect, back_button, draw_button, draw_footer,
    draw_header_until, header_button, one_line,
};
use super::{App, View};
use crate::canvas::Canvas;
use crate::collate::turkish_lower;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};
use crate::text::Family;

/// En fazla bu kadar sonuç toplanır.
const MAX_HITS: usize = 300;
/// Sonucun önünde ve ardında gösterilen karakter sayısı.
const CONTEXT: usize = 40;
const NEW_SEARCH_W: i32 = 260;

pub(super) struct Hit {
    pub spine: usize,
    pub start: Position,
    pub end: Position,
    pub snippet: String,
}

pub(super) struct BookSearch {
    pub query: String,
    pub hits: Vec<Hit>,
    /// Sonuçlar `MAX_HITS`'te kesildi.
    pub truncated: bool,
}

/// Arama için harf: küçük, şapkasız; Türkçe harfler Latin eşleriyle aynı
/// (ç=c, ş=s, ğ=g, ö=o, ü=u, ı=i), klavyeden özel harf yazmadan aransın.
pub(super) fn fold(c: char) -> char {
    match turkish_lower(c) {
        'â' => 'a',
        'î' | 'ı' => 'i',
        'û' | 'ü' => 'u',
        'ç' => 'c',
        'ş' => 's',
        'ğ' => 'g',
        'ö' => 'o',
        c => c,
    }
}

/// `text` içinde (katlanmış) `query`'nin geçtiği bayt aralıkları.
pub(super) fn find_all(text: &str, query: &[char]) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    let chars: Vec<(usize, char)> = text.char_indices().map(|(i, c)| (i, fold(c))).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + query.len() <= chars.len() {
        if chars[i..i + query.len()]
            .iter()
            .map(|(_, c)| *c)
            .eq(query.iter().copied())
        {
            let end = chars.get(i + query.len()).map_or(text.len(), |(b, _)| *b);
            out.push((chars[i].0, end));
            i += query.len();
        } else {
            i += 1;
        }
    }
    out
}

/// Bulunan yerin çevresi, tek satır.
fn snippet(text: &str, start: usize, end: usize) -> String {
    let before: String = {
        let mut v: Vec<char> = text[..start].chars().rev().take(CONTEXT + 1).collect();
        let cut = v.len() > CONTEXT;
        v.truncate(CONTEXT);
        v.reverse();
        (if cut { "…" } else { "" }).to_string() + &v.into_iter().collect::<String>()
    };
    let after: String = {
        let v: Vec<char> = text[end..].chars().take(CONTEXT + 1).collect();
        let cut = v.len() > CONTEXT;
        v.into_iter().take(CONTEXT).collect::<String>() + if cut { "…" } else { "" }
    };
    format!("{before}{}{after}", &text[start..end])
}

impl App {
    /// Bütün kitapta arar (bölümler sırayla okunur; cihazda büyük kitapta birkaç saniye sürebilir).
    pub(super) fn search_book(&mut self, query: &str) -> Option<BookSearch> {
        let s = self.session.as_mut()?;
        let q: Vec<char> = query.trim().chars().map(fold).collect();
        let mut hits = Vec::new();
        let mut truncated = false;
        let started = std::time::Instant::now();
        'chapters: for spine in 0..s.chapter_count() {
            let blocks = match &mut s.doc {
                Doc::Epub(book) => book.chapter(spine).unwrap_or_default(),
                Doc::Pdf(pdf) => pdf.page_blocks(spine).unwrap_or_default(),
            };
            for (block, b) in blocks.iter().enumerate() {
                for (a, e) in find_all(&b.text, &q) {
                    if hits.len() == MAX_HITS {
                        truncated = true;
                        break 'chapters;
                    }
                    hits.push(Hit {
                        spine,
                        start: Position { block, offset: a },
                        end: Position { block, offset: e },
                        snippet: snippet(&b.text, a, e),
                    });
                }
            }
        }
        log::info!(
            "search {query:?}: {} hits in {:?}",
            hits.len(),
            started.elapsed()
        );
        Some(BookSearch {
            query: query.trim().to_string(),
            hits,
            truncated,
        })
    }

    /// Sonucun bölüm etiketi: içindekilerdeki başlık, yoksa "Bölüm N"; PDF'te "Sayfa N".
    fn chapter_label(&self, spine: usize) -> String {
        let Some(s) = &self.session else {
            return String::new();
        };
        if s.pdf().is_some() {
            return tr(&Msg::PageNumber(spine + 1));
        }
        s.toc()
            .iter()
            .rev()
            .find(|t| t.spine_index <= spine)
            .map_or_else(|| tr(&Msg::ChapterNumber(spine + 1)), |t| t.title.clone())
    }

    /// Arama formu; son aranan kelimeyle dolu.
    pub(super) fn open_search_form(&mut self) -> Option<Refresh> {
        let last = self.search.as_ref().map_or("", |s| s.query.as_str());
        let field = super::form::Field::new(Msg::FieldQuery, last);
        self.open_form(super::form::Form::new(
            super::form::FormKind::BookSearch,
            vec![field],
        ))
    }

    fn new_search_button(&self) -> Rect {
        header_button(self.width, NEW_SEARCH_W)
    }

    /// Sonuç açıkken sol üstte "← Sonuçlar" (bağlantı geri düğmesi yoksa).
    pub(super) fn results_button(&self) -> Option<Rect> {
        (self.search_mark.is_some()
            && self.link_back_button().is_none()
            && matches!(self.view, View::Reading))
        .then_some(Rect {
            x: MARGIN,
            y: 10,
            w: 260,
            h: 70,
        })
    }

    pub(super) fn open_hit(&mut self, index: usize) -> Option<Refresh> {
        let hit = self.search.as_ref()?.hits.get(index)?;
        let (spine, start, end) = (hit.spine, hit.start, hit.end);
        self.open_chapter(spine, Target::At(start));
        if let Some(s) = self.session.as_mut() {
            s.load_page_text();
        }
        // PDF görüntüsünde sayfa birden çok ekrana bölünmüş olabilir.
        if let Some(screen) = self.screen_of_hit(start, end)
            && let Some(s) = self.session.as_mut()
        {
            s.page = screen;
        }
        self.search_mark = Some((spine, start, end));
        Some(self.refresh(true))
    }

    /// PDF sayfa görüntüsünde sonucun ilk karakterini gösteren ekran.
    fn screen_of_hit(&self, start: Position, end: Position) -> Option<usize> {
        let s = self.session.as_ref().filter(|s| s.showing_pdf_image())?;
        let first = *box_areas(s.pdf_text.as_ref()?, start, end).first()?;
        s.pages.iter().position(|p| {
            p.screen.is_some_and(|sc| {
                let a = sc.area;
                (a.x..a.x + a.w).contains(&first.x) && (a.y..a.y + a.h).contains(&first.y)
            })
        })
    }

    pub(super) fn handle_search_results(&mut self, page: usize, event: Event) -> Option<Refresh> {
        let len = self.search.as_ref().map_or(0, |s| s.hits.len());
        if let Event::Tap { x, y } = event {
            if back_button(self.width).contains(x, y) {
                self.search_mark = None;
                self.view = View::Reading;
                return Some(self.refresh(true));
            }
            if self.new_search_button().contains(x, y) {
                return self.open_search_form();
            }
            let index = SEARCH.row_at(page, len, y)?;
            return self.open_hit(index);
        }
        let page = Self::page_list(page, SEARCH.pages(len), event)?;
        self.view = View::SearchResults { page };
        Some(self.refresh(false))
    }

    pub(super) fn draw_search_results(&self, canvas: &mut Canvas, page: usize) {
        let Some(search) = &self.search else {
            return;
        };
        let t = &self.text;
        let new_search = self.new_search_button();
        let count = if search.truncated {
            Msg::SearchHitsTruncated(search.hits.len())
        } else {
            Msg::SearchHits(search.hits.len())
        };
        draw_header_until(
            t,
            canvas,
            &tr(&Msg::SearchTitle(search.query.clone())),
            &tr(&count),
            new_search.x - 30,
        );
        draw_button(t, canvas, new_search, &tr(&Msg::NewSearch), ROW_PX);
        draw_button(
            t,
            canvas,
            back_button(canvas.width()),
            &tr(&Msg::Back),
            ROW_PX,
        );
        if search.hits.is_empty() {
            self.draw_message_lines(canvas, &Msg::NoSearchHits, HEADER_BOTTOM + 80);
        }
        let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
        for (index, row) in SEARCH.visible(page, search.hits.len()) {
            let hit = &search.hits[index];
            let r = SEARCH.row_rect(canvas.width(), row);
            let snippet = one_line(t, &hit.snippet, PLAIN, ROW_PX, max_w);
            t.draw_plain(
                canvas,
                &snippet,
                Family::Ui,
                PLAIN,
                MARGIN,
                r.y + 50,
                ROW_PX,
            );
            let label = one_line(t, &self.chapter_label(hit.spine), BOLD, 30.0, max_w);
            t.draw_plain(canvas, &label, Family::Ui, BOLD, MARGIN, r.y + 95, 30.0);
        }
        let footer = Msg::ListFooter {
            page: page + 1,
            pages: SEARCH.pages(search.hits.len()),
        };
        draw_footer(t, canvas, &footer, MARGIN);
    }

    pub(super) fn draw_results_button(&self, canvas: &mut Canvas) {
        if let Some(r) = self.results_button() {
            draw_button(&self.text, canvas, r, &tr(&Msg::BackToResults), ROW_PX);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(s: &str) -> Vec<char> {
        s.chars().map(fold).collect()
    }

    #[test]
    fn matches_ignore_turkish_case_circumflex_and_special_letters() {
        let text = "İstanbul'da kâğıt; ISPARTA ve ıspanak. istanbul";
        let hits = find_all(text, &q("istanbul"));
        assert_eq!(hits.len(), 2);
        assert_eq!(&text[hits[0].0..hits[0].1], "İstanbul");
        assert_eq!(&text[hits[1].0..hits[1].1], "istanbul");
        assert_eq!(find_all(text, &q("kağıt")).len(), 1, "şapka fark etmez");
        assert_eq!(find_all(text, &q("kagit")).len(), 1, "ğ=g, ı=i");
        assert_eq!(find_all("Burçak tarlası", &q("burcak")).len(), 1, "ç=c");
        // Noktalı/noktasız i aynı: "isparta" ISPARTA'yı, "index" "Index"i bulur.
        assert_eq!(find_all(text, &q("isparta")).len(), 1);
        assert_eq!(find_all(text, &q("ıspanak")).len(), 1);
        assert_eq!(find_all("Index", &q("index")).len(), 1);
        assert!(find_all(text, &[]).is_empty());
    }

    #[test]
    fn snippets_show_the_surroundings() {
        let text = format!("{}çay{}", "a".repeat(50), "b".repeat(10));
        let (a, e) = find_all(&text, &q("çay"))[0];
        let s = snippet(&text, a, e);
        assert!(
            s.starts_with('…') && s.contains("çaybbbbbbbbbb") && !s.ends_with('…'),
            "{s}"
        );
        // Öndeki 40 "a" ve "çay"daki "a".
        assert_eq!(s.chars().filter(|c| *c == 'a').count(), CONTEXT + 1);
    }
}
