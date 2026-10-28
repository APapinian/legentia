//! Sözlük maddelerini okuyucunun bloklarına çevirir: kalın, italik ve
//! paragraflar korunur, diğer biçimlendirme atılır. Sözlük HTML'i çoğu zaman
//! düzgün XML değildir; bu yüzden hata vermeyen basit bir etiket tarayıcısı kullanılır.
//!
//! - HTML, Pango, XDXF: `<b>`, `<strong>`, `<i>`, `<em>`, XDXF `<k>` (madde başı)
//!   ve `<ex>` (örnek); `<br>`, `<p>`, `<div>`, `<li>`… yeni paragraf.
//! - DSL (Lingvo): `[b]`, `[i]`, `[ex]`; `[m1]`…`[m9]` ve satır sonu yeni paragraf;
//!   `[s]` (ses/resim dosyası) ve `{{yorum}}` atılır; `\[` kaçışı.

use super::{Article, Markup};
use crate::epub::{Block, BlockKind};
use crate::text::{Span, Style};

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    text: String,
    spans: Vec<Span>,
    bold: u32,
    italic: u32,
}

impl Builder {
    fn style(&self) -> Style {
        Style {
            bold: self.bold > 0,
            italic: self.italic > 0,
            link: false,
        }
    }

    /// Metin ekler; boşluk dizileri tek boşluğa iner, paragraf başında boşluk olmaz.
    fn push(&mut self, s: &str) {
        let style = self.style();
        for c in s.chars() {
            if c.is_whitespace() {
                if self.text.is_empty() || self.text.ends_with(' ') {
                    continue;
                }
                self.text.push(' ');
            } else {
                self.text.push(c);
            }
            let end = self.text.len();
            match self.spans.last_mut() {
                Some(last) if last.style == style => last.end = end,
                _ => self.spans.push(Span {
                    start: self.spans.last().map_or(0, |s| s.end),
                    end,
                    style,
                }),
            }
        }
    }

    fn paragraph(&mut self) {
        self.close(BlockKind::Paragraph);
    }

    fn close(&mut self, kind: BlockKind) {
        let len = self.text.trim_end().len();
        self.text.truncate(len);
        for s in &mut self.spans {
            s.end = s.end.min(len);
        }
        self.spans.retain(|s| s.start < s.end);
        if !self.text.is_empty() {
            self.blocks.push(Block {
                kind,
                text: std::mem::take(&mut self.text),
                spans: std::mem::take(&mut self.spans),
            });
        }
        self.text.clear();
        self.spans.clear();
    }
}

/// Bir sözlüğün maddelerinden bloklar: madde başı (başlık), sözlük adı (italik), içerik.
pub fn article_blocks(dict: &str, article: &Article) -> Vec<Block> {
    let mut b = Builder::default();
    b.push(&article.headword);
    b.close(BlockKind::Heading(2));
    b.italic += 1;
    b.push(dict);
    b.italic -= 1;
    b.paragraph();
    for (markup, text) in &article.parts {
        match markup {
            Markup::Plain => {
                for line in text.lines() {
                    b.push(line);
                    b.paragraph();
                }
            }
            Markup::Dsl => dsl(&mut b, text),
            Markup::Html | Markup::Pango | Markup::Xdxf => {
                tags(&mut b, text, *markup != Markup::Html)
            }
        }
        b.paragraph();
    }
    b.blocks
}

/// `<etiket>` biçimli metin. `newlines`: satır sonları da paragraf böler (Pango, XDXF).
fn tags(b: &mut Builder, text: &str, newlines: bool) {
    let mut rest = text;
    while !rest.is_empty() {
        let (plain, tag) = match rest.find('<') {
            Some(i) => {
                let end = rest[i..].find('>').map_or(rest.len(), |e| i + e + 1);
                (&rest[..i], Some(&rest[i..end]))
            }
            None => (rest, None),
        };
        {
            let decoded = entities(plain);
            if newlines {
                let mut lines = decoded.split('\n').peekable();
                while let Some(line) = lines.next() {
                    b.push(line);
                    if lines.peek().is_some() {
                        b.paragraph();
                    }
                }
            } else {
                b.push(&decoded);
            }
        }
        rest = &rest[plain.len()..];
        let Some(tag) = tag else { break };
        rest = &rest[tag.len()..];
        let inner = tag.trim_start_matches('<').trim_end_matches('>').trim();
        let closing = inner.starts_with('/');
        let name: String = inner
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        let self_closing = inner.ends_with('/');
        match name.as_str() {
            // İçerik atlanır; içinde "<" olabileceğinden doğrudan kapanış aranır.
            "script" | "style" | "head" if !closing && !self_closing => {
                let close = format!("</{name}");
                rest = match rest.to_ascii_lowercase().find(&close) {
                    Some(i) => rest[i..].find('>').map_or("", |e| &rest[i + e + 1..]),
                    None => "",
                };
            }
            "b" | "strong" | "k" => toggle(&mut b.bold, closing),
            "i" | "em" | "ex" => toggle(&mut b.italic, closing),
            "br" | "p" | "div" | "li" | "tr" | "blockquote" | "h1" | "h2" | "h3" | "h4" | "h5"
            | "h6" | "def" | "dd" | "dt" => b.paragraph(),
            _ => {}
        }
    }
}

fn toggle(depth: &mut u32, closing: bool) {
    if closing {
        *depth = depth.saturating_sub(1);
    } else {
        *depth += 1;
    }
}

/// HTML varlıkları: adlı olanların sık kullanılanları ve sayısal olanlar.
pub(crate) fn entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(semi) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..semi];
        let c = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => name
                .strip_prefix("#x")
                .or_else(|| name.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// DSL (Lingvo) biçimlendirmesi.
fn dsl(b: &mut Builder, text: &str) {
    let mut chars = text.chars().peekable();
    let mut plain = String::new();
    let mut skip = 0u32; // [s]…[/s] içinde
    let flush = |b: &mut Builder, plain: &mut String, skip: u32| {
        if skip == 0 {
            b.push(plain);
        }
        plain.clear();
    };
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    plain.push(n);
                }
            }
            '{' if chars.peek() == Some(&'{') => {
                // {{yorum}} atılır.
                let mut prev = ' ';
                for n in chars.by_ref() {
                    if prev == '}' && n == '}' {
                        break;
                    }
                    prev = n;
                }
            }
            '\n' => {
                flush(b, &mut plain, skip);
                b.paragraph();
            }
            '[' => {
                flush(b, &mut plain, skip);
                let tag: String = chars.by_ref().take_while(|&c| c != ']').collect();
                let closing = tag.starts_with('/');
                let name: String = tag
                    .trim_start_matches('/')
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '*')
                    .collect();
                match name.as_str() {
                    "b" => toggle(&mut b.bold, closing),
                    "i" | "ex" => toggle(&mut b.italic, closing),
                    "s" => toggle(&mut skip, closing),
                    m if m.starts_with('m') && !closing => b.paragraph(),
                    _ => {}
                }
            }
            _ => plain.push(c),
        }
    }
    flush(b, &mut plain, skip);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paragraf metni ve (kalın, italik, parça) listesi.
    type Rendered = Vec<(String, Vec<(bool, bool, String)>)>;

    fn render(markup: Markup, text: &str) -> Rendered {
        let a = Article {
            headword: "çay".into(),
            parts: vec![(markup, text.into())],
        };
        article_blocks("S", &a)
            .into_iter()
            .skip(2)
            .map(|b| {
                let spans = b
                    .spans
                    .iter()
                    .map(|s| {
                        (
                            s.style.bold,
                            s.style.italic,
                            b.text[s.start..s.end].to_string(),
                        )
                    })
                    .collect();
                (b.text, spans)
            })
            .collect()
    }

    fn texts(markup: Markup, text: &str) -> Vec<String> {
        render(markup, text).into_iter().map(|(t, _)| t).collect()
    }

    #[test]
    fn header_blocks_and_span_coverage() {
        let a = Article {
            headword: "Çay".into(),
            parts: vec![(
                Markup::Html,
                "<b>1.</b> İçecek &amp; bitki<br>2. Akarsu".into(),
            )],
        };
        let blocks = article_blocks("Türkçe Sözlük", &a);
        assert_eq!(blocks[0].kind, BlockKind::Heading(2));
        assert_eq!(blocks[1].text, "Türkçe Sözlük");
        for block in &blocks {
            // Biçimler metni boşluksuz, sırayla kaplar.
            assert_eq!(block.spans.first().unwrap().start, 0);
            assert_eq!(block.spans.last().unwrap().end, block.text.len());
            assert!(block.spans.windows(2).all(|w| w[0].end == w[1].start));
        }
        assert_eq!(blocks[2].text, "1. İçecek & bitki");
        assert_eq!(blocks[3].text, "2. Akarsu");
    }

    #[test]
    fn html_styles_entities_and_scripts() {
        let r = render(
            Markup::Html,
            "<div><b>isim</b>  <i>örnek&nbsp;cümle</i>&#305;</div><script>x<y</script><p>son &#x15F;</p>",
        );
        assert_eq!(r[0].0, "isim örnek cümleı");
        assert_eq!(r[0].1[0], (true, false, "isim".into()));
        assert_eq!(r[0].1[2], (false, true, "örnek cümle".into()));
        assert_eq!(r[1].0, "son ş");
        assert_eq!(r.len(), 2, "betik içeriği atılır");
    }

    #[test]
    fn broken_html_does_not_fail() {
        assert_eq!(
            texts(Markup::Html, "<b>kapanmamış <i>etiket"),
            vec!["kapanmamış etiket"]
        );
        assert_eq!(texts(Markup::Html, "a < b & c"), vec!["a"]);
        assert_eq!(
            texts(Markup::Html, "x &bilinmeyen; y"),
            vec!["x &bilinmeyen; y"]
        );
    }

    #[test]
    fn dsl_markup() {
        let r = render(
            Markup::Dsl,
            "\t[m1][b]1.[/b] [trn]sıcak içecek[/trn][/m]\n\t[m2][ex]bir \\[bardak\\] çay[/ex][/m] [s]cay.wav[/s]{{not}}",
        );
        assert_eq!(r[0].0, "1. sıcak içecek");
        assert_eq!(r[0].1[0], (true, false, "1.".into()));
        assert_eq!(r[1].0, "bir [bardak] çay");
        assert!(r[1].1[0].1, "örnek italik");
    }

    #[test]
    fn pango_and_xdxf_keep_line_breaks() {
        assert_eq!(texts(Markup::Pango, "<b>a</b>\nb"), vec!["a", "b"]);
        assert_eq!(
            texts(Markup::Xdxf, "<k>çay</k>\n<ex>demli çay</ex>"),
            vec!["çay", "demli çay"]
        );
    }
}
