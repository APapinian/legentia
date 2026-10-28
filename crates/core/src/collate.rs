//! Türkçe alfabetik sıralama ve büyük/küçük harf dönüşümü.
//! Standart kütüphane `İ/ı`'yı bilmez: `"I".to_lowercase()` "i" verir, Türkçede "ı" olmalı.

/// Türk alfabesi; â, î, û kendi ünlülerinin hemen ardından, q/w/x Latin sırasına göre.
const ORDER: &str = "aâbcçdefgğhıiîjklmnoöpqrsştuüûvwxyz";

/// Türkçe küçük harf: `I` → `ı`, `İ` → `i`.
pub fn turkish_lower(c: char) -> char {
    match c {
        'I' => 'ı',
        'İ' => 'i',
        _ => c.to_lowercase().next().unwrap_or(c),
    }
}

/// Türkçe büyük harf: `i` → `İ`, `ı` → `I`. Bazı harfler birden çok harfe
/// dönüşebildiği için (ß → SS) dizgi döner.
pub fn turkish_upper(c: char) -> String {
    match c {
        'i' => "İ".into(),
        'ı' => "I".into(),
        _ => c.to_uppercase().collect(),
    }
}

/// Sıralama anahtarı: boşluklar önce, sonra rakamlar, sonra Türk alfabesi,
/// en sonda diğer karakterler. Noktalama yok sayılır.
pub fn sort_key(s: &str) -> Vec<u32> {
    s.chars()
        .filter_map(|c| {
            let c = turkish_lower(c);
            if c.is_whitespace() {
                Some(1)
            } else if let Some(d) = c.to_digit(10) {
                Some(10 + d)
            } else if let Some(i) = ORDER.chars().position(|o| o == c) {
                Some(100 + i as u32)
            } else if c.is_alphanumeric() {
                Some(1000 + c as u32)
            } else {
                None
            }
        })
        .collect()
}

/// Türkçe sıraya göre karşılaştırma; anahtarlar eşitse özgün metne bakılır.
pub fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    sort_key(a).cmp(&sort_key(b)).then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_by_turkish_alphabet() {
        let mut words = vec![
            "Zeynep", "şeker", "Işık", "Çiçek", "İnce", "ılık", "ceviz", "Ömer", "Uğur", "Ünal",
            "Oya",
        ];
        words.sort_by(|a, b| compare(a, b));
        assert_eq!(
            words,
            vec![
                "ceviz", "Çiçek", "ılık", "Işık", "İnce", "Oya", "Ömer", "şeker", "Uğur", "Ünal",
                "Zeynep"
            ]
        );
    }

    #[test]
    fn dotted_and_dotless_i_lowercase_correctly() {
        assert_eq!(turkish_lower('I'), 'ı');
        assert_eq!(turkish_lower('İ'), 'i');
        assert_eq!(turkish_lower('Ş'), 'ş');
    }

    #[test]
    fn punctuation_is_ignored_and_digits_come_first() {
        let mut titles = vec!["“Beyaz Geceler”", "1984", "Ağaçlar"];
        titles.sort_by(|a, b| compare(a, b));
        assert_eq!(titles, vec!["1984", "Ağaçlar", "“Beyaz Geceler”"]);
    }
}
