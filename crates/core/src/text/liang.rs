//! Türkçe dışı diller için heceleme: Liang algoritması ve TeX hyph-utf8
//! desenleri (İngilizce, Almanca, Fransızca, İspanyolca, İtalyanca; lisanslar
//! `crates/core/hyph/LISANSLAR.md`). Desenler uygulamaya gömülüdür, bir dil
//! ilk kez hecelenirken ayrıştırılır.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Bir dilin desenleri ve kelimenin başında/sonunda bırakılacak en az harf.
pub(crate) struct Patterns {
    /// Harfler → harflerin arasındaki (ve iki ucundaki) değerler.
    values: HashMap<Vec<char>, Vec<u8>>,
    longest: usize,
    /// Hazır bölünmüş kelimeler: kelime → bölünme noktaları (harf sırası).
    exceptions: HashMap<String, Vec<usize>>,
    left: usize,
    right: usize,
}

struct Language {
    tag: &'static str,
    patterns: &'static str,
    exceptions: &'static str,
    left: usize,
    right: usize,
}

/// Desen dosyalarındaki "hyphenmins" değerleriyle.
const LANGUAGES: [Language; 5] = [
    Language {
        tag: "en",
        patterns: include_str!("../../hyph/en-us.pat.txt"),
        exceptions: include_str!("../../hyph/en-us.hyp.txt"),
        left: 2,
        right: 3,
    },
    Language {
        tag: "de",
        patterns: include_str!("../../hyph/de-1996.pat.txt"),
        exceptions: "",
        left: 2,
        right: 2,
    },
    Language {
        tag: "fr",
        patterns: include_str!("../../hyph/fr.pat.txt"),
        exceptions: "",
        left: 2,
        right: 2,
    },
    Language {
        tag: "es",
        patterns: include_str!("../../hyph/es.pat.txt"),
        exceptions: "",
        left: 2,
        right: 2,
    },
    Language {
        tag: "it",
        patterns: include_str!("../../hyph/it.pat.txt"),
        exceptions: "",
        left: 2,
        right: 2,
    },
];

/// Kitabın dil etiketine ("en", "en-GB", "de-DE"…) göre desenler; desteklenmeyen dilde `None`.
pub(crate) fn for_language(lang: &str) -> Option<&'static Patterns> {
    static PARSED: [OnceLock<Patterns>; 5] = [const { OnceLock::new() }; 5];
    let primary = lang.split(['-', '_']).next()?.to_ascii_lowercase();
    let i = LANGUAGES.iter().position(|l| l.tag == primary)?;
    Some(PARSED[i].get_or_init(|| {
        let l = &LANGUAGES[i];
        Patterns::parse(l.patterns, l.exceptions, l.left, l.right)
    }))
}

impl Patterns {
    fn parse(patterns: &str, exceptions: &str, left: usize, right: usize) -> Patterns {
        let mut values = HashMap::new();
        let mut longest = 0;
        for p in patterns.split_whitespace() {
            let mut letters = Vec::new();
            let mut v = vec![0u8];
            for c in p.chars() {
                match c.to_digit(10) {
                    Some(d) => *v.last_mut().expect("en az bir değer") = d as u8,
                    None => {
                        letters.push(c);
                        v.push(0);
                    }
                }
            }
            longest = longest.max(letters.len());
            values.insert(letters, v);
        }
        let exceptions = exceptions
            .split_whitespace()
            .map(|e| {
                let mut points = Vec::new();
                let mut n = 0;
                for c in e.chars() {
                    if c == '-' {
                        points.push(n);
                    } else {
                        n += 1;
                    }
                }
                (e.replace('-', ""), points)
            })
            .collect();
        Patterns {
            values,
            longest,
            exceptions,
            left,
            right,
        }
    }

    /// Kelimenin bölünebileceği harf sıraları (o harften önce bölünür).
    fn word(&self, word: &[char]) -> Vec<usize> {
        let n = word.len();
        if n < self.left + self.right {
            return Vec::new();
        }
        let lower: Vec<char> = word
            .iter()
            .map(|c| c.to_lowercase().next().unwrap_or(*c))
            .collect();
        let points = match self.exceptions.get(&lower.iter().collect::<String>()) {
            Some(p) => p.clone(),
            None => {
                let mut w = vec!['.'];
                w.extend(&lower);
                w.push('.');
                // values[i]: w[i-1] ile w[i] arası.
                let mut values = vec![0u8; w.len() + 1];
                for i in 0..w.len() {
                    for j in i + 1..=(i + self.longest).min(w.len()) {
                        if let Some(v) = self.values.get(&w[i..j]) {
                            for (k, &d) in v.iter().enumerate() {
                                values[i + k] = values[i + k].max(d);
                            }
                        }
                    }
                }
                // Kelimenin k. harfinden önceki sınır: w[k] ile w[k+1] arası.
                (1..n).filter(|&k| values[k + 1] % 2 == 1).collect()
            }
        };
        points
            .into_iter()
            .filter(|&k| k >= self.left && n - k >= self.right)
            .collect()
    }

    /// `text` içinde tire konabilecek bayt konumları; yalnızca harf dizileri
    /// hecelenir (Türkçe hecelemeyle aynı sözleşme).
    pub(crate) fn hyphenate(&self, text: &str) -> Vec<usize> {
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            if !chars[i].1.is_alphabetic() {
                i += 1;
                continue;
            }
            let start = i;
            while i < chars.len() && chars[i].1.is_alphabetic() {
                i += 1;
            }
            let word: Vec<char> = chars[start..i].iter().map(|(_, c)| *c).collect();
            out.extend(self.word(&word).into_iter().map(|k| chars[start + k].0));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(lang: &str, word: &str) -> String {
        let p = for_language(lang).unwrap();
        let mut out = String::new();
        let points = p.hyphenate(word);
        for (i, c) in word.char_indices() {
            if points.contains(&i) {
                out.push('-');
            }
            out.push(c);
        }
        out
    }

    #[test]
    fn words_are_split_like_tex() {
        // Knuth'un TeXbook örnekleri ve desen dosyasındaki istisna.
        assert_eq!(split("en", "hyphenation"), "hy-phen-ation");
        assert_eq!(split("en-GB", "associate"), "as-so-ciate");
        // Sağda en az 3 harf kalır (İngilizce kuralı): "put-er" bölünmez.
        assert_eq!(split("en", "Computer"), "Com-puter", "büyük harf korunur");
        assert_eq!(split("de-DE", "Silbentrennung"), "Sil-ben-tren-nung");
        assert_eq!(split("fr", "bibliothèque"), "bi-blio-thèque");
        assert_eq!(split("es", "biblioteca"), "bi-blio-te-ca");
        assert_eq!(split("it", "biblioteca"), "bi-blio-te-ca");
    }

    #[test]
    fn short_words_punctuation_and_unknown_languages() {
        assert_eq!(split("en", "the"), "the");
        assert_eq!(split("en", "(hyphenation)."), "(hy-phen-ation).");
        assert!(for_language("ru").is_none());
        assert!(for_language("").is_none());
    }
}
