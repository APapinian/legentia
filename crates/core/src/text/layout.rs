//! Paragraf düzeni: Unicode satır kırma noktalarında açgözlü doldurma,
//! sığmayan kelimede Türkçe heceleme, ardından hizalama.

use unicode_linebreak::linebreaks;

use super::fonts::{Family, Style};
use super::{Glyph, Text, hyphen, liang};

/// Paragraf metninin `[start, end)` bayt aralığının biçimi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub style: Style,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Align {
    Left,
    Justify,
    Center,
}

pub(crate) struct ParaStyle<'a> {
    pub family: Family,
    pub px: f32,
    /// Tüm paragrafı kalın yap (başlıklar).
    pub bold: bool,
    pub align: Align,
    pub hyphenate: bool,
    pub lang: Option<&'a str>,
}

/// Bir satır: glifler ve satırın sol kenarına göre x konumları.
/// `start..end` satırın paragraf metnindeki bayt aralığıdır (sondaki boşluk dahil).
pub(crate) struct Line {
    pub glyphs: Vec<(Glyph, f32)>,
    pub start: usize,
    pub end: usize,
}

/// İki yana yaslarken bir boşluğun en fazla kaç katına genişleyebileceği;
/// daha fazlası çirkin boşluklar bırakır, o satır sola yaslanır.
const MAX_SPACE_STRETCH: f32 = 3.0;

/// `spans` metnin tamamını sırayla ve boşluksuz kaplamalıdır.
pub(crate) fn layout(
    t: &Text,
    text: &str,
    spans: &[Span],
    style: &ParaStyle,
    max_w: f32,
) -> Vec<Line> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut glyphs = Vec::new();
    for span in spans {
        let face = t.fonts().face(
            style.family,
            Style {
                bold: span.style.bold || style.bold,
                ..span.style
            },
        );
        t.shape(
            &text[span.start..span.end],
            face,
            style.px,
            style.lang,
            span.style.link,
            span.start,
            &mut glyphs,
        );
    }
    // prefix[i]: ilk i glifin toplam genişliği.
    let mut prefix = Vec::with_capacity(glyphs.len() + 1);
    prefix.push(0.0f32);
    for g in &glyphs {
        prefix.push(prefix.last().copied().unwrap_or(0.0) + g.advance);
    }
    let glyph_at = |byte: usize| glyphs.partition_point(|g| g.cluster < byte);
    let width = |a: usize, b: usize| prefix[glyph_at(b)] - prefix[glyph_at(a)];
    let trim = |a: usize, b: usize| a + text[a..b].trim_end().len();
    let is_cluster_start = |byte: usize| {
        glyphs
            .get(glyph_at(byte))
            .is_some_and(|g| g.cluster == byte)
    };

    let breaks: Vec<usize> = linebreaks(text).map(|(i, _)| i).collect();
    // Türkçe (ya da dili bilinmeyen) kitap Türkçe kurallarıyla, desteklenen diğer
    // diller TeX desenleriyle hecelenir; desteklenmeyen dilde heceleme yok.
    let turkish = style.lang.is_none_or(|l| l.starts_with("tr"));
    let patterns = style
        .lang
        .filter(|_| !turkish)
        .and_then(liang::for_language);
    let hyphenate = style.hyphenate && (turkish || patterns.is_some());

    let mut lines = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut last_fit = None;
        let mut overflow = None;
        for &p in breaks.iter().filter(|&&p| p > start) {
            if width(start, trim(start, p)) <= max_w {
                last_fit = Some(p);
            } else {
                overflow = Some(p);
                break;
            }
        }
        let Some(over) = overflow else {
            lines.push(make_line(
                &glyphs,
                text,
                start,
                text.len(),
                None,
                max_w,
                style.align,
                true,
            ));
            break;
        };
        let word_start = last_fit.unwrap_or(start);

        // Sığmayan kelimeyi hecelemeyi dene.
        let mut cut = None;
        if hyphenate {
            let word_end = trim(word_start, over);
            let word = &text[word_start..word_end];
            let points = match patterns {
                Some(p) => p.hyphenate(word),
                None => hyphen::turkish(word),
            };
            for h in points.into_iter().rev() {
                let h = word_start + h;
                if !is_cluster_start(h) {
                    continue;
                }
                let hyphen_w = hyphen_glyph(t, &glyphs[glyph_at(h) - 1]).advance;
                if width(start, h) + hyphen_w <= max_w {
                    cut = Some(h);
                    break;
                }
            }
        }
        if let Some(h) = cut {
            let hy = hyphen_glyph(t, &glyphs[glyph_at(h) - 1]);
            lines.push(make_line(
                &glyphs,
                text,
                start,
                h,
                Some(hy),
                max_w,
                style.align,
                false,
            ));
            start = h;
        } else if let Some(p) = last_fit {
            lines.push(make_line(
                &glyphs,
                text,
                start,
                p,
                None,
                max_w,
                style.align,
                false,
            ));
            start = p;
        } else {
            // Satırdan uzun tek kelime: sığan son glif kümesinde böl (en az bir küme).
            let first = glyph_at(start);
            let mut end_glyph = first + 1;
            while end_glyph < glyphs.len()
                && glyphs[end_glyph].cluster < over
                && prefix[end_glyph + 1] - prefix[first] <= max_w
            {
                end_glyph += 1;
            }
            let end = glyphs.get(end_glyph).map_or(text.len(), |g| g.cluster);
            lines.push(make_line(
                &glyphs,
                text,
                start,
                end,
                None,
                max_w,
                style.align,
                false,
            ));
            start = end;
        }
    }
    lines
}

/// Önceki glifle aynı yüz ve boyutta bir kısa çizgi.
fn hyphen_glyph(t: &Text, before: &Glyph) -> Glyph {
    let mut out = Vec::new();
    t.shape(
        "-",
        before.face,
        before.px,
        None,
        false,
        before.cluster,
        &mut out,
    );
    out.into_iter().next().unwrap_or(Glyph {
        advance: 0.0,
        ..before.clone()
    })
}

/// Sondaki boşluklar çizilmez ve genişliğe katılmaz.
#[allow(clippy::too_many_arguments)]
fn make_line(
    glyphs: &[Glyph],
    text: &str,
    start: usize,
    end: usize,
    hyphen: Option<Glyph>,
    max_w: f32,
    align: Align,
    last: bool,
) -> Line {
    let visible_end = start + text[start..end].trim_end().len();
    let from = glyphs.partition_point(|g| g.cluster < start);
    let to = glyphs.partition_point(|g| g.cluster < visible_end);
    let mut placed: Vec<(Glyph, f32)> = Vec::with_capacity(to - from + 1);
    let mut pen = 0.0;
    for g in &glyphs[from..to] {
        placed.push((g.clone(), pen));
        pen += g.advance;
    }
    if let Some(h) = hyphen {
        let w = h.advance;
        placed.push((h, pen));
        pen += w;
    }

    match align {
        Align::Justify if !last => {
            let spaces: Vec<usize> = (0..placed.len())
                .filter(|&i| {
                    text[placed[i].0.cluster..].starts_with(' ')
                        && placed[i].0.cluster < visible_end
                })
                .collect();
            if let Some(space_w) = spaces.first().map(|&i| placed[i].0.advance) {
                let extra = (max_w - pen) / spaces.len() as f32;
                if extra > 0.0 && extra <= space_w * MAX_SPACE_STRETCH {
                    let mut shift = 0.0;
                    let mut next_space = spaces.iter().peekable();
                    for (i, (_, x)) in placed.iter_mut().enumerate() {
                        *x += shift;
                        if next_space.peek() == Some(&&i) {
                            next_space.next();
                            shift += extra;
                        }
                    }
                }
            }
        }
        Align::Center => {
            let offset = ((max_w - pen) / 2.0).max(0.0);
            for (_, x) in &mut placed {
                *x += offset;
            }
        }
        _ => {}
    }
    Line {
        glyphs: placed,
        start,
        end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::test_text;

    const TEXT: &str = "Ağaçların gölgesinde oturup çayımızı içerken, ışığın yaprakların arasından süzülüşünü \
        sessizce izledik. Kardeşlerimizden biri, İstanbul'dan getirdiği eski bir kitabı okuyordu.";

    fn para(align: Align, hyphenate: bool) -> ParaStyle<'static> {
        ParaStyle {
            family: Family::Book(0),
            px: 40.0,
            bold: false,
            align,
            hyphenate,
            lang: Some("tr"),
        }
    }

    fn plain(text: &str) -> Vec<Span> {
        vec![Span {
            start: 0,
            end: text.len(),
            style: Style::default(),
        }]
    }

    /// Satırın çizilen genişliği: son glifin sağ kenarı.
    fn width(l: &Line) -> f32 {
        l.glyphs.last().map_or(0.0, |(g, x)| x + g.advance)
    }

    fn run(text: &str, style: &ParaStyle, max_w: f32) -> Vec<Line> {
        layout(&test_text(), text, &plain(text), style, max_w)
    }

    #[test]
    fn lines_fit_and_cover_the_whole_text() {
        let lines = run(TEXT, &para(Align::Left, true), 700.0);
        assert!(lines.len() > 2);
        assert_eq!(lines[0].start, 0);
        assert_eq!(lines.last().unwrap().end, TEXT.len());
        for pair in lines.windows(2) {
            assert_eq!(
                pair[0].end, pair[1].start,
                "satırlar arasında boşluk/çakışma"
            );
        }
        for l in &lines {
            assert!(width(l) <= 700.0 + 0.5, "taşan satır: {}", width(l));
        }
    }

    #[test]
    fn justified_lines_reach_the_margin_except_the_last() {
        let lines = run(TEXT, &para(Align::Justify, true), 700.0);
        for l in &lines[..lines.len() - 1] {
            assert!(
                (width(l) - 700.0).abs() < 0.5,
                "yaslanmamış satır: {}",
                width(l)
            );
        }
        assert!(width(lines.last().unwrap()) < 700.0);
    }

    #[test]
    fn narrow_column_hyphenates_turkish_words() {
        let t = test_text();
        let lines = layout(&t, TEXT, &plain(TEXT), &para(Align::Left, true), 330.0);
        let hyphenated: Vec<_> = lines
            .iter()
            .filter(|l| !TEXT[..l.end].ends_with(' ') && l.end < TEXT.len())
            .collect();
        assert!(!hyphenated.is_empty(), "dar sütunda hiç heceleme yapılmadı");
        // Bölünen her yerde satır sonunda kısa çizgi glifi olmalı.
        let mut dash = Vec::new();
        t.shape(
            "-",
            t.fonts().face(Family::Book(0), Style::default()),
            40.0,
            None,
            false,
            0,
            &mut dash,
        );
        for l in hyphenated {
            let prev = TEXT[..l.end].chars().last().unwrap();
            if prev.is_alphabetic() {
                assert_eq!(l.glyphs.last().unwrap().0.id, dash[0].id);
            }
        }
    }

    #[test]
    fn other_languages_use_their_patterns_and_unknown_ones_are_not_hyphenated() {
        let t = test_text();
        let text = "Characteristically, international organizations communicate electronically.";
        let split = |lang: &'static str| {
            let style = ParaStyle {
                lang: Some(lang),
                ..para(Align::Left, true)
            };
            // Her kelime tek başına sığar (sığmayan kelime harf harf bölünürdü).
            layout(&t, text, &plain(text), &style, 480.0)
                .iter()
                .filter(|l| l.end < text.len() && !text[..l.end].ends_with(' '))
                .map(|l| l.end)
                .collect::<Vec<_>>()
        };
        let english = split("en");
        assert!(!english.is_empty(), "İngilizce desenlerle bölündü");
        // Bölünmeler İngilizce desenlerin izin verdiği yerlerde.
        let allowed = super::liang::for_language("en").unwrap().hyphenate(text);
        assert!(english.iter().all(|e| allowed.contains(e)), "{english:?}");
        assert!(split("ru").is_empty(), "desteklenmeyen dilde heceleme yok");
    }

    #[test]
    fn no_hyphenation_when_disabled() {
        // En uzun kelime sütuna sığmalı; sığmayan kelime harf harf bölünür (ayrı test).
        let longest = run("Kardeşlerimizden", &para(Align::Left, false), 10_000.0);
        assert!(width(&longest[0]) < 450.0);
        let lines = run(TEXT, &para(Align::Left, false), 450.0);
        for l in &lines[..lines.len() - 1] {
            let next = TEXT[l.end..].chars().next().unwrap();
            let prev = TEXT[..l.end].chars().last().unwrap();
            assert!(
                !(prev.is_alphabetic() && next.is_alphabetic()),
                "kelime ortasında bölündü"
            );
        }
    }

    #[test]
    fn word_longer_than_line_is_split_without_loss() {
        let w = "Çekoslovakyalılaştıramadıklarımızdanmışsınız";
        let lines = run(w, &para(Align::Left, false), 250.0);
        assert!(lines.len() > 1);
        assert_eq!(lines.last().unwrap().end, w.len());
    }

    #[test]
    fn turkish_language_tag_reaches_the_shaper() {
        // Literata'da 'fi' bitişik harfi (ligatür) var; Türkçede "i"nin noktası
        // korunmalı, bu yüzden tr etiketiyle iki ayrı glif bekleriz.
        let t = test_text();
        let face = t.fonts().face(Family::Book(0), Style::default());
        let (mut tr, mut en) = (Vec::new(), Vec::new());
        t.shape("fi", face, 40.0, Some("tr"), false, 0, &mut tr);
        t.shape("fi", face, 40.0, Some("en"), false, 0, &mut en);
        assert_eq!(tr.len(), 2, "Türkçede fi ligatürü uygulanmamalı");
        assert!(en.len() <= tr.len());
    }
}
