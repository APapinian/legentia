//! Türkçe heceleme (TDK kuralları). Türkçede her hecede tek ünlü bulunur:
//! iki ünlü arasındaki son ünsüz sonraki heceye geçer (a-ra-ba, kar-deş,
//! Türk-çe, elekt-rik); yan yana iki ünlü arasından bölünür (sa-at).
//! Satır sonunda tek harf kalmaması için her iki yanda en az iki harf aranır.
//! q, w, x içeren kelimeler yabancı sayılır ve bölünmez.

const MIN_LEFT: usize = 2;
const MIN_RIGHT: usize = 2;

fn is_vowel(c: char) -> bool {
    matches!(
        c,
        'a' | 'e'
            | 'ı'
            | 'i'
            | 'o'
            | 'ö'
            | 'u'
            | 'ü'
            | 'â'
            | 'î'
            | 'û'
            | 'A'
            | 'E'
            | 'I'
            | 'İ'
            | 'O'
            | 'Ö'
            | 'U'
            | 'Ü'
            | 'Â'
            | 'Î'
            | 'Û'
    )
}

/// `text` içinde tire konabilecek bayt konumları (o konumdan önce bölünür).
/// Yalnızca harf dizileri hecelenir; noktalama ve kesme işareti sınır sayılır.
pub(crate) fn turkish(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
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
        let letters = &chars[start..i];
        // Türkçede olmayan harfler yabancı kelimeye işaret eder (Luxembourg,
        // Wagner); Türkçe kurallarıyla bölmek yanlış olur.
        if !letters
            .iter()
            .any(|(_, c)| matches!(c, 'q' | 'w' | 'x' | 'Q' | 'W' | 'X'))
        {
            word(letters, &mut out);
        }
    }
    out
}

fn word(chars: &[(usize, char)], out: &mut Vec<usize>) {
    let vowels: Vec<usize> = (0..chars.len()).filter(|&k| is_vowel(chars[k].1)).collect();
    for pair in vowels.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let cut = if b - a == 1 { b } else { b - 1 };
        if cut >= MIN_LEFT && chars.len() - cut >= MIN_RIGHT {
            out.push(chars[cut].0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(w: &str) -> String {
        let mut s = String::new();
        let cuts = turkish(w);
        for (i, c) in w.char_indices() {
            if cuts.contains(&i) {
                s.push('-');
            }
            s.push(c);
        }
        s
    }

    #[test]
    fn follows_tdk_rules() {
        assert_eq!(show("kardeşlerimizden"), "kar-deş-le-ri-miz-den");
        assert_eq!(show("Türkçe"), "Türk-çe");
        assert_eq!(show("elektrik"), "elekt-rik");
        assert_eq!(show("program"), "prog-ram");
        assert_eq!(show("saatler"), "sa-at-ler");
        assert_eq!(show("İstanbul"), "İs-tan-bul");
        assert_eq!(show("ÇEKOSLOVAKYA"), "ÇE-KOS-LO-VAK-YA");
    }

    #[test]
    fn keeps_two_letters_on_each_side() {
        assert_eq!(show("araba"), "ara-ba");
        assert_eq!(show("okul"), "okul");
        assert_eq!(show("ev"), "ev");
    }

    #[test]
    fn foreign_words_are_not_hyphenated() {
        assert_eq!(show("Luxembourg"), "Luxembourg");
        assert_eq!(show("Wagner'in"), "Wagner'in");
    }

    #[test]
    fn punctuation_and_apostrophe_are_boundaries() {
        assert_eq!(show("“Ankara'dan,"), "“An-ka-ra'dan,");
        assert_eq!(show("gel-git"), "gel-git");
    }
}
