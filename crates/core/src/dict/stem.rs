//! Türkçe kök bulma (kendi kurallarımız). Çekimli kelimeden sondan ek soyarak
//! aday kökler üretilir; hangisinin gerçek kök olduğuna sözlük karar verir
//! (sözlükte bulunan ilk aday). Ekler fazla soyulsa da sözlükte olmayan aday
//! elenir; az soyulmuş (uzun) aday önce gelir: "gözlüğü" → "gözlük", "göz" değil.
//!
//! Soyulan ekten sonra kökün eski hâli de denenir:
//! - ünsüz yumuşaması: kitab-ı → kitap, reng-i → renk, gid-iyor → git
//! - ünlü düşmesi: oğl-u → oğul, burn-u → burun
//! - çift ünsüz: hakk-ı → hak
//! - -yor öncesi daralma: başl-ıyor → başla
//!
//! Fiil eki soyulduysa kökün mastarı (-mak/-mek) kökten önce gelir; sözlüklerde
//! fiiller mastar hâlinde yazılır.

use std::collections::HashSet;
use std::sync::OnceLock;

use unicode_normalization::UnicodeNormalization;

use crate::collate::turkish_lower;

/// En fazla bu kadar ek soyulur ve bu kadar aday üretilir.
const MAX_DEPTH: usize = 8;
const MAX_CANDIDATES: usize = 300;

/// Kökte kalması gereken en az harf sayısı.
const MIN_STEM: usize = 2;

const BACK: &str = "aıou";
const FRONT: &str = "eiöü";

struct Suffix {
    form: String,
    /// Fiil eki: kök fiildir, mastarı da aranır.
    verbal: bool,
    /// -Iyor: öncesindeki a/e daralıp düşmüş olabilir.
    yor: bool,
}

/// Ek kalıpları. Büyük harfler ses değişimidir: A = a/e, I = ı/i/u/ü,
/// D = d/t, C = c/ç; parantez içi (y), (n), (s), (I), (A) kaynaştırma ya da
/// ünlüdür, olabilir de olmayabilir de.
const NOUN: &[&str] = &[
    // çokluk, iyelik
    "lAr", "(I)m", "(I)n", "(s)I", "(I)mIz", "(I)nIz", "lArI", // hâl
    "(y)I", "(y)A", "DA", "DAn", "(n)In", "(y)lA", "CA", "ki", "nA", "nDA", "nDAn", "nI",
    // yapım (sık)
    "lIk", "lI", "sIz", "CI", // ek-fiil ve kişi
    "DIr", "(y)DI", "(y)mIş", "(y)sA", "(y)ken", "(y)Im", "sIn", "(y)Iz", "sInIz", "k", "m", "n",
];
const VERB: &[&str] = &[
    "(y)AcAk", "(y)AcAğ", "mIş", "DI", "(A)r", "(I)r", "mAz", "mA", "mAlI", "sA", "(y)Ip",
    "(y)ArAk", "(y)IncA", "(y)An", "DIk", "DIğ", "mAk", "mAktA", "mAsI", "(y)Abil", "(y)AmA",
];

/// Kalıbın bütün yüzey biçimleri: "(y)DI" → "dı", "ydı", "di", … "tü".
fn expand(pattern: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        let options: Vec<String> = if c == '(' {
            let inner: String = chars.by_ref().take_while(|&c| c != ')').collect();
            let mut v = expand(&inner);
            v.push(String::new());
            v
        } else {
            match c {
                'A' => vec!["a".into(), "e".into()],
                'I' => vec!["ı".into(), "i".into(), "u".into(), "ü".into()],
                'D' => vec!["d".into(), "t".into()],
                'C' => vec!["c".into(), "ç".into()],
                _ => vec![c.to_string()],
            }
        };
        out = out
            .iter()
            .flat_map(|o| options.iter().map(move |p| format!("{o}{p}")))
            .collect();
    }
    out
}

fn suffixes() -> &'static [Suffix] {
    static LIST: OnceLock<Vec<Suffix>> = OnceLock::new();
    LIST.get_or_init(|| {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let tagged = NOUN
            .iter()
            .map(|p| (p, false))
            .chain(VERB.iter().map(|p| (p, true)));
        for (pattern, verbal) in tagged.chain([(&"(I)yor", true)]) {
            for form in expand(pattern) {
                if !form.is_empty() && seen.insert((form.clone(), verbal)) {
                    out.push(Suffix {
                        yor: form.ends_with("yor"),
                        form,
                        verbal,
                    });
                }
            }
        }
        out
    })
}

fn is_vowel(c: char) -> bool {
    BACK.contains(c) || FRONT.contains(c)
}

fn last_vowel(s: &str) -> Option<char> {
    s.chars().rev().find(|&c| is_vowel(c))
}

/// Son ünlüye uyan dar ünlü (ı/i/u/ü) ve geniş ünlü (a/e).
fn harmony(s: &str) -> (char, char) {
    match last_vowel(s) {
        Some('a' | 'ı') => ('ı', 'a'),
        Some('o' | 'u') => ('u', 'a'),
        Some('e' | 'i') => ('i', 'e'),
        Some('ö' | 'ü') => ('ü', 'e'),
        _ => ('i', 'e'),
    }
}

/// Ek soyulduktan sonra kökün olası eski hâlleri (kendisi dahil).
fn repairs(stem: &str, suffix: &Suffix) -> Vec<String> {
    let mut out = vec![stem.to_string()];
    let chars: Vec<char> = stem.chars().collect();
    let Some(&last) = chars.last() else {
        return out;
    };
    let vowel_initial =
        suffix.form.chars().next().is_some_and(is_vowel) || suffix.form.starts_with('y');
    if suffix.yor && !is_vowel(last) {
        let (_, wide) = harmony(stem);
        out.push(format!("{stem}{wide}"));
    }
    if vowel_initial && !is_vowel(last) {
        let hard = match last {
            'b' => Some('p'),
            'c' => Some('ç'),
            'd' => Some('t'),
            'ğ' | 'g' => Some('k'),
            _ => None,
        };
        if let Some(h) = hard {
            out.push(chars[..chars.len() - 1].iter().chain([&h]).collect());
        }
        if chars.len() >= 3 {
            let before = chars[chars.len() - 2];
            if before == last {
                out.push(chars[..chars.len() - 1].iter().collect());
            } else if !is_vowel(before) {
                // Düşmüş dar ünlüyü geri koy: burn → burun.
                let (narrow, _) = harmony(stem);
                let mut s: String = chars[..chars.len() - 1].iter().collect();
                s.push(narrow);
                s.push(last);
                out.push(s);
            }
        }
    }
    out
}

/// Mastar: gel → gelmek, başla → başlamak.
fn infinitive(stem: &str) -> String {
    let (_, wide) = harmony(stem);
    format!("{stem}m{wide}k")
}

/// Kelimenin Türkçe küçük harfli, NFC biçimi.
pub fn normalize(word: &str) -> String {
    word.nfc().map(turkish_lower).collect()
}

/// Sözlükte sırayla aranacak adaylar: önce kelimenin kendisi, sonra az
/// soyulmuştan çok soyulmuşa kökler. Kesme işaretli özel adda (İstanbul'dan)
/// kesmeden önceki kısım da en başta denenir.
pub fn candidates(word: &str) -> Vec<String> {
    let word =
        normalize(word.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '’'));
    let mut out: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |s: String, out: &mut Vec<String>| {
        if !s.is_empty() && seen.insert(s.clone()) {
            out.push(s);
        }
    };
    let joined: String = word.chars().filter(|c| !matches!(c, '\'' | '’')).collect();
    if let Some((head, _)) = word.split_once(['\'', '’']) {
        push(head.to_string(), &mut out);
    }
    push(joined.clone(), &mut out);

    // Genişlik öncelikli: önce bir ek soyulmuşlar, sonra iki…
    let mut level = vec![(joined, false)];
    for _ in 0..MAX_DEPTH {
        let mut next = Vec::new();
        for (s, verbal) in &level {
            for suffix in suffixes() {
                let Some(stem) = s.strip_suffix(suffix.form.as_str()) else {
                    continue;
                };
                if stem.chars().count() < MIN_STEM || last_vowel(stem).is_none() {
                    continue;
                }
                let verbal = *verbal || suffix.verbal;
                for r in repairs(stem, suffix) {
                    next.push((r, verbal));
                }
            }
        }
        for (s, verbal) in &next {
            if *verbal {
                push(infinitive(s), &mut out);
            }
            push(s.clone(), &mut out);
            if out.len() >= MAX_CANDIDATES {
                return out;
            }
        }
        if next.is_empty() {
            break;
        }
        next.sort();
        next.dedup();
        level = next;
    }
    out
}

/// `exists` doğru dönen ilk aday.
pub fn lookup_form(word: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    candidates(word).into_iter().find(|c| exists(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DICT: &[&str] = &[
        "kitap",
        "gelmek",
        "gel",
        "başlamak",
        "beklemek",
        "oğul",
        "burun",
        "ağız",
        "hak",
        "renk",
        "göz",
        "gözlük",
        "yaz",
        "yazmak",
        "istanbul",
        "ev",
        "okumak",
        "gitmek",
        "yapmak",
        "su",
        "anne",
        "an",
        "kardeş",
        "gölge",
        "ağaç",
        "söylemek",
        "güzel",
        "isim",
        "çocuk",
    ];

    fn found(word: &str) -> Option<String> {
        lookup_form(word, |c| DICT.contains(&c))
    }

    #[test]
    fn inflected_words_reach_their_dictionary_form() {
        for (word, root) in [
            ("kitaplarımızdan", "kitap"),
            ("Kitabı", "kitap"),
            ("geliyorum", "gelmek"),
            ("başlıyor", "başlamak"),
            ("bekliyordu", "beklemek"),
            ("oğlu", "oğul"),
            ("burnundan", "burun"),
            ("ağzımız", "ağız"),
            ("hakkında", "hak"),
            ("rengi", "renk"),
            ("gözlüğü", "gözlük"),
            ("gözlerimiz", "göz"),
            ("yazıyorum", "yazmak"),
            ("İstanbul'dan", "istanbul"),
            ("evlerinizde", "ev"),
            ("okuyacağız", "okumak"),
            ("gidiyor", "gitmek"),
            ("yapmadı", "yapmak"),
            ("suyu", "su"),
            ("anneme", "anne"),
            ("kardeşlerimizden", "kardeş"),
            ("ağaçların", "ağaç"),
            ("gölgesinde", "gölge"),
            ("söylemişti", "söylemek"),
            ("güzeldi", "güzel"),
            ("ismi", "isim"),
            ("çocuğu", "çocuk"),
        ] {
            assert_eq!(found(word).as_deref(), Some(root), "{word}");
        }
    }

    #[test]
    fn a_word_in_the_dictionary_is_not_stripped() {
        assert_eq!(found("anne").as_deref(), Some("anne"));
        assert_eq!(found("gözlük").as_deref(), Some("gözlük"));
        assert_eq!(found("yaz").as_deref(), Some("yaz"));
    }

    #[test]
    fn unknown_words_find_nothing_and_stay_bounded() {
        assert_eq!(found("qwxz"), None);
        assert!(
            candidates("çekoslovakyalılaştıramadıklarımızdanmışsınızcasına").len()
                <= MAX_CANDIDATES
        );
    }

    #[test]
    fn patterns_expand_to_all_surface_forms() {
        let mut v = expand("(y)DI");
        v.sort();
        assert_eq!(v.len(), 16);
        assert!(v.contains(&"ydü".to_string()) && v.contains(&"tı".to_string()));
    }
}
