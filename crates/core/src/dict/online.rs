//! Çevrimiçi sözlükler: TDK Güncel Türkçe Sözlük ve Vikisözlük. Bu modül
//! yalnızca istek adreslerini kurar ve yanıtları maddeye çevirir; ağ isteği
//! uygulama katmanında, arka planda yapılır.
//!
//! - TDK: `sozluk.gov.tr/gts` (sitenin kendi kullandığı, belgelenmemiş uç;
//!   resmî API ya da kullanım şartı yayımlanmamış). Yalnızca sözlükteki biçimi
//!   tanır; çekimli kelimenin kökleri sırayla denenir.
//! - Vikisözlük: MediaWiki API'si, içerik CC BY-SA. Çekimli biçimlerin de
//!   sayfası olduğundan önce bütün adayların kategorisine bakılır ve
//!   "Türkçe sözcükler" kategorisindeki (asıl madde) ilk aday seçilir.

use serde_json::Value;

use super::{Article, DictError, Markup};
use crate::i18n::{Msg, tr};

/// TDK'da en fazla bu kadar aday (kelime ve kökleri) denenir; her biri bir istek.
pub const TDK_TRIES: usize = 6;
/// Vikisözlük'e tek istekte sorulan en fazla aday (API sınırı 50).
pub const WIKTIONARY_TITLES: usize = 20;
/// Sağlayıcıların adresleri; istek kuran fonksiyonlar bunları (testlerde
/// yerel sunucuyu) `base` olarak alır.
pub const TDK: &str = "https://sozluk.gov.tr/gts";
pub const WIKTIONARY: &str = "https://tr.wiktionary.org/w/api.php";
/// Vikisözlük'te asıl maddeleri çekim sayfalarından ayıran kategori.
const LEMMA_CATEGORY: &str = "Kategori:Türkçe sözcükler";

fn invalid(e: impl std::fmt::Display) -> DictError {
    DictError::Invalid(e.to_string())
}

/// Yüzde kodlama (RFC 3986 ayrılmamış karakterler dışında her bayt).
fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("").trim()
}

// --- TDK ---

pub fn tdk_url(base: &str, word: &str) -> String {
    format!("{base}?ara={}", encode(word))
}

/// GTS yanıtı: eş sesli her madde ayrı. Kelime yoksa boş liste
/// (`{"error": "Sonuç bulunamadı"}`).
pub fn parse_tdk(json: &[u8]) -> Result<Vec<Article>, DictError> {
    let v: Value = serde_json::from_slice(json).map_err(invalid)?;
    let Some(entries) = v.as_array() else {
        return match v.get("error") {
            Some(_) => Ok(Vec::new()),
            None => Err(invalid("unexpected TDK response")),
        };
    };
    Ok(entries.iter().map(tdk_article).collect())
}

fn tdk_article(m: &Value) -> Article {
    let mut html = String::new();
    let origin = text(m, "lisan");
    if !origin.is_empty() {
        html.push_str(&format!("<p><i>{}</i></p>", escape(origin)));
    }
    let mut meanings: Vec<&Value> = m
        .get("anlamlarListe")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    meanings.sort_by_key(|a| text(a, "anlam_sira").parse::<u32>().unwrap_or(0));
    for (i, a) in meanings.iter().enumerate() {
        let features: Vec<&str> = a
            .get("ozelliklerListe")
            .and_then(Value::as_array)
            .map(|l| {
                l.iter()
                    .map(|o| text(o, "tam_adi"))
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        html.push_str(&format!("<p><b>{}.</b> ", i + 1));
        if !features.is_empty() {
            html.push_str(&format!("<i>{}</i> ", escape(&features.join(", "))));
        }
        // TDK "bakınız" için ► kullanır; fontlarda yok.
        html.push_str(&escape(&text(a, "anlam").replace('►', "→")));
        html.push_str("</p>");
        for o in a
            .get("orneklerListe")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let author = o
                .get("yazar")
                .and_then(Value::as_array)
                .and_then(|y| y.first())
                .map(|y| text(y, "tam_adi"))
                .unwrap_or("");
            let mut example = format!("“{}”", text(o, "ornek"));
            if !author.is_empty() {
                example.push_str(&format!(" — {author}"));
            }
            html.push_str(&format!("<p><i>{}</i></p>", escape(&example)));
        }
    }
    let compounds = text(m, "birlesikler");
    if !compounds.is_empty() {
        html.push_str(&format!(
            "<p><b>{}:</b> {}</p>",
            tr(&Msg::CompoundWords),
            escape(compounds)
        ));
    }
    let idioms: Vec<&str> = m
        .get("atasozu")
        .and_then(Value::as_array)
        .map(|l| {
            l.iter()
                .map(|a| text(a, "madde"))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if !idioms.is_empty() {
        html.push_str(&format!(
            "<p><b>{}:</b> {}</p>",
            tr(&Msg::Idioms),
            escape(&idioms.join(", "))
        ));
    }
    Article {
        headword: text(m, "madde").to_string(),
        parts: vec![(Markup::Html, html)],
    }
}

// --- Vikisözlük ---

/// Adayların hangisinin Türkçe asıl madde olduğunu soran istek (tek istek).
pub fn wiktionary_lemma_url(base: &str, candidates: &[String]) -> String {
    let titles: Vec<&str> = candidates
        .iter()
        .take(WIKTIONARY_TITLES)
        .map(String::as_str)
        .collect();
    format!(
        "{base}?action=query&format=json&formatversion=2&redirects=1&prop=categories&clcategories={}&titles={}",
        encode(LEMMA_CATEGORY),
        encode(&titles.join("|"))
    )
}

/// Sırayla ilk Türkçe asıl madde olan aday ve sayfa adı (yönlendirme izlenmiş).
pub fn pick_lemma(
    json: &[u8],
    candidates: &[String],
) -> Result<Option<(String, String)>, DictError> {
    let v: Value = serde_json::from_slice(json).map_err(invalid)?;
    let q = v
        .get("query")
        .ok_or_else(|| invalid("no query in response"))?;
    let renamed = |list: &str, title: &str| -> Option<String> {
        q.get(list)?
            .as_array()?
            .iter()
            .find(|r| text(r, "from") == title)
            .map(|r| text(r, "to").to_string())
    };
    let pages = q
        .get("pages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for c in candidates.iter().take(WIKTIONARY_TITLES) {
        let mut title = renamed("normalized", c).unwrap_or_else(|| c.clone());
        if let Some(to) = renamed("redirects", &title) {
            title = to;
        }
        let lemma = pages.iter().any(|p| {
            text(p, "title") == title
                && p.get("missing").is_none()
                && p.get("categories")
                    .and_then(Value::as_array)
                    .is_some_and(|cs| cs.iter().any(|c| text(c, "title") == LEMMA_CATEGORY))
        });
        if lemma {
            return Ok(Some((c.clone(), title)));
        }
    }
    Ok(None)
}

pub fn wiktionary_extract_url(base: &str, title: &str) -> String {
    format!(
        "{base}?action=query&format=json&formatversion=2&prop=extracts&explaintext=1&titles={}",
        encode(title)
    )
}

/// Sayfanın Türkçe bölümü; `== Başlık ==` satırları kalın, diğerleri paragraf.
pub fn parse_wiktionary(json: &[u8], title: &str) -> Result<Option<Article>, DictError> {
    let v: Value = serde_json::from_slice(json).map_err(invalid)?;
    let extract = v
        .pointer("/query/pages")
        .and_then(Value::as_array)
        .and_then(|p| p.iter().find(|p| text(p, "title") == title))
        .map(|p| text(p, "extract").to_string())
        .unwrap_or_default();
    let Some(start) = extract.find("== Türkçe ==") else {
        return Ok(None);
    };
    let section = &extract[start + "== Türkçe ==".len()..];
    // Sonraki dil bölümü ("== Azerice ==") Türkçe bölümünü bitirir.
    let end = section
        .lines()
        .scan(0, |at, line| {
            let pos = *at;
            *at += line.len() + 1;
            Some((pos, line))
        })
        .find(|(_, l)| l.starts_with("== ") && l.ends_with(" =="))
        .map_or(section.len(), |(pos, _)| pos);
    let mut html = String::new();
    for line in section[..end]
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        if line.starts_with("==") && line.ends_with("==") {
            let heading = line.trim_matches('=').trim();
            html.push_str(&format!("<p><b>{}</b></p>", escape(heading)));
        } else {
            html.push_str(&format!("<p>{}</p>", escape(line)));
        }
    }
    if html.is_empty() {
        return Ok(None);
    }
    Ok(Some(Article {
        headword: title.to_string(),
        parts: vec![(Markup::Html, html)],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict::render::article_blocks;

    /// sozluk.gov.tr/gts?ara=göz yanıtından kısaltılmış.
    const TDK_GOZ: &str = r#"[{"madde":"göz","lisan":"","birlesikler":"göz akı, gözyaşı",
      "anlamlarListe":[
        {"anlam_sira":"2","anlam":"Bazı deyimlerde, görme ve bakma",
         "orneklerListe":[{"ornek":"Gözden geçirmek.","yazar_id":"0"}]},
        {"anlam_sira":"1","anlam":"Görme organı; basar, ayn",
         "ozelliklerListe":[{"tam_adi":"isim"},{"tam_adi":"anatomi"}]},
        {"anlam_sira":"3","anlam":"► çekmece & göz",
         "orneklerListe":[{"ornek":"Köprünün gözleri karış karış kazılmıştır.",
           "yazar":[{"tam_adi":"Sait Faik Abasıyanık"}]}]}],
      "atasozu":[{"madde":"göz göre göre"},{"madde":"gözü pek"}]},
      {"madde":"göz","lisan":"Farsça","anlamlarListe":[{"anlam_sira":"1","anlam":"İkinci madde"}]}]"#;

    fn plain(article: &Article) -> String {
        article_blocks("TDK", article)
            .iter()
            .map(|b| b.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn tdk_entries_become_numbered_articles() {
        let articles = parse_tdk(TDK_GOZ.as_bytes()).unwrap();
        assert_eq!(articles.len(), 2, "eş sesliler ayrı madde");
        let text = plain(&articles[0]);
        assert!(
            text.contains("1. isim, anatomi Görme organı; basar, ayn"),
            "{text}"
        );
        assert!(text.contains("2. Bazı deyimlerde"), "sırası anlam_sira'dan");
        assert!(
            text.contains("“Köprünün gözleri karış karış kazılmıştır.” — Sait Faik Abasıyanık")
        );
        assert!(text.contains("→ çekmece & göz"), "& kaçışlı, ► okunur");
        assert!(text.contains("Birleşik kelimeler: göz akı, gözyaşı"));
        assert!(text.contains("göz göre göre, gözü pek"));
        assert!(plain(&articles[1]).contains("Farsça"));
        assert!(
            parse_tdk(r#"{"error":"Sonuç bulunamadı"}"#.as_bytes())
                .unwrap()
                .is_empty()
        );
        assert!(parse_tdk(b"<html>").is_err());
    }

    #[test]
    fn wiktionary_skips_inflected_pages_and_reads_the_turkish_section() {
        let candidates: Vec<String> = ["gözler", "Göz", "göz"].map(String::from).to_vec();
        let url = wiktionary_lemma_url(WIKTIONARY, &candidates);
        assert!(
            url.contains("titles=g%C3%B6zler%7CG%C3%B6z%7Cg%C3%B6z"),
            "{url}"
        );
        let lemmas = r#"{"query":{
            "normalized":[{"from":"Göz","to":"Göz"}],
            "pages":[
              {"title":"gözler"},
              {"title":"Göz","missing":true},
              {"title":"göz","categories":[{"title":"Kategori:Türkçe sözcükler"}]}]}}"#;
        assert_eq!(
            pick_lemma(lemmas.as_bytes(), &candidates).unwrap(),
            Some(("göz".to_string(), "göz".to_string())),
            "çekim sayfası (kategorisiz) atlanır"
        );
        let none = r#"{"query":{"pages":[{"title":"gözler"}]}}"#;
        assert_eq!(pick_lemma(none.as_bytes(), &candidates).unwrap(), None);

        let extract = r#"{"query":{"pages":[{"title":"göz","extract":"\n== Türkçe ==\n\n\n=== Köken ===\nOsmanlı Türkçesi sözcüğünden.\n\n=== Ad ===\n(organlar) Görme organı.\n\n== Azerice ==\nBaşka dil.\n"}]}}"#;
        let article = parse_wiktionary(extract.as_bytes(), "göz")
            .unwrap()
            .unwrap();
        let text = plain(&article);
        assert!(
            text.contains("Köken") && text.contains("(organlar) Görme organı."),
            "{text}"
        );
        assert!(
            !text.contains("Azerice") && !text.contains("Başka dil"),
            "{text}"
        );
        let other = r#"{"query":{"pages":[{"title":"x","extract":"== Azerice ==\nyok"}]}}"#;
        assert_eq!(parse_wiktionary(other.as_bytes(), "x").unwrap(), None);
    }
}
